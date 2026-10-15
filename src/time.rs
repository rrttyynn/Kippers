use std::{
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crate::thread::ThreadData;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Limits {
    Infinite,
    Depth(i32),
    Time(u64),
    Nodes(u64),
    Mate(u64),
    Fischer(u64, u64),
    Cyclic(u64, u64, u64),
}

const TIME_OVERHEAD_MS: u64 = 15;

// Shared by the command listener and all workers for one UCI search.
// Modified for native pondering in Kippers, October 15, 2026.
pub struct SearchControl {
    pondering: AtomicBool,
    stopped: AtomicBool,
    hit_time: OnceLock<Instant>,
    wake: Mutex<()>,
    changed: Condvar,
}

impl SearchControl {
    pub fn new(pondering: bool) -> Self {
        Self {
            pondering: AtomicBool::new(pondering),
            stopped: AtomicBool::new(false),
            hit_time: OnceLock::new(),
            wake: Mutex::new(()),
            changed: Condvar::new(),
        }
    }

    pub fn is_pondering(&self) -> bool {
        self.pondering.load(Ordering::Acquire)
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    pub fn ponderhit(&self) {
        let _guard = self.wake.lock().unwrap();
        if self.is_pondering() && !self.is_stopped() {
            // Publish the clock before enabling normal time limits.
            let _ = self.hit_time.set(Instant::now());
            self.pondering.store(false, Ordering::Release);
            self.changed.notify_all();
        }
    }

    pub fn stop(&self) {
        let _guard = self.wake.lock().unwrap();
        self.stopped.store(true, Ordering::Release);
        self.pondering.store(false, Ordering::Release);
        self.changed.notify_all();
    }

    pub fn wait_for_release(&self) {
        let mut guard = self.wake.lock().unwrap();
        while self.is_pondering() {
            guard = self.changed.wait(guard).unwrap();
        }
    }
}

#[derive(Clone)]
pub struct TimeManager {
    limits: Limits,
    start_time: Instant,
    soft_bound: Duration,
    hard_bound: Duration,
    ponder: Option<Arc<SearchControl>>,
}

impl TimeManager {
    pub fn new(limits: Limits, fullmove_number: usize, move_overhead: u64) -> Self {
        let soft;
        let hard;

        match limits {
            Limits::Time(ms) => {
                soft = ms;
                hard = ms;
            }
            Limits::Fischer(main, inc) => {
                let soft_scale = 0.0594 - 0.0492 * (-0.0386 * fullmove_number as f64).exp();
                let hard_scale = 0.7281;

                let soft_bound = (soft_scale * main.saturating_sub(move_overhead) as f64 + 0.75 * inc as f64) as u64;
                let hard_bound = (hard_scale * main.saturating_sub(move_overhead) as f64 + 0.75 * inc as f64) as u64;

                soft = soft_bound.min(main.saturating_sub(move_overhead));
                hard = hard_bound.min(main.saturating_sub(move_overhead));
            }
            Limits::Cyclic(main, inc, moves) => {
                let main = main.saturating_sub(move_overhead);
                let base = (main as f64 / moves as f64) + 0.75 * inc as f64;

                soft = ((1.0 * base) as u64).min(main + inc);
                hard = ((5.0 * base) as u64).min(main + inc);
            }
            _ => {
                soft = u64::MAX;
                hard = u64::MAX;
            }
        }

        Self {
            limits,
            start_time: Instant::now(),
            soft_bound: Duration::from_millis(soft.saturating_sub(TIME_OVERHEAD_MS)),
            hard_bound: Duration::from_millis(hard.saturating_sub(TIME_OVERHEAD_MS)),
            ponder: None,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn with_ponder(mut self, control: Arc<SearchControl>) -> Self {
        self.ponder = Some(control);
        self
    }

    fn is_pondering(&self) -> bool {
        self.ponder.as_ref().is_some_and(|control| control.is_pondering())
    }

    fn budget_elapsed(&self) -> Duration {
        self.ponder.as_ref().and_then(|control| control.hit_time.get()).unwrap_or(&self.start_time).elapsed()
    }

    pub fn soft_limit(&self, td: &ThreadData, multiplier: impl Fn() -> f32) -> bool {
        if self.is_pondering() {
            return false;
        }
        match self.limits {
            Limits::Infinite | Limits::Depth(_) | Limits::Mate(_) => false,
            Limits::Nodes(maximum) => td.shared.nodes.aggregate() >= maximum,
            Limits::Time(maximum) => self.budget_elapsed() >= Duration::from_millis(maximum),
            _ => self.budget_elapsed() >= Duration::from_secs_f32(self.soft_bound.as_secs_f32() * multiplier()),
        }
    }

    pub fn check_time(&self, td: &ThreadData) -> bool {
        if td.completed_depth == 0 {
            return false;
        }

        match self.limits {
            Limits::Infinite | Limits::Depth(_) | Limits::Mate(_) => false,
            Limits::Nodes(maximum) => td.shared.nodes.aggregate() > maximum,
            _ => td.nodes() & 2047 == 2047 && !self.is_pondering() && self.budget_elapsed() >= self.hard_bound,
        }
    }

    pub fn limits(&self) -> Limits {
        self.limits.clone()
    }

    pub fn use_time_management(&self) -> bool {
        matches!(self.limits, Limits::Fischer(..) | Limits::Cyclic(..) | Limits::Time(_)) && !self.is_pondering()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ponderhit_restarts_budget_clock_but_keeps_total_search_time() {
        let control = Arc::new(SearchControl::new(true));
        let mut tm = TimeManager::new(Limits::Time(500), 1, 50).with_ponder(control.clone());
        tm.start_time = Instant::now() - Duration::from_secs(10);
        let clone = tm.clone();
        assert!(!tm.use_time_management());
        control.ponderhit();
        assert!(tm.use_time_management());
        assert!(tm.elapsed() >= Duration::from_secs(10));
        assert!(tm.budget_elapsed() < Duration::from_secs(1));
        assert!(clone.budget_elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn duplicate_or_idle_ponderhit_does_not_reset_clock() {
        let control = SearchControl::new(true);
        control.ponderhit();
        let first = *control.hit_time.get().unwrap();
        control.ponderhit();
        assert_eq!(*control.hit_time.get().unwrap(), first);
        let idle = SearchControl::new(false);
        idle.ponderhit();
        assert!(idle.hit_time.get().is_none());
    }

    #[test]
    fn stop_wakes_ponder_waiter_and_prevents_late_hit() {
        let control = Arc::new(SearchControl::new(true));
        let waiting = control.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            waiting.wait_for_release();
            tx.send(()).unwrap();
        });
        control.stop();
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
        control.ponderhit();
        assert!(control.is_stopped());
        assert!(!control.is_pondering());
        assert!(control.hit_time.get().is_none());
    }

    #[test]
    fn hit_wakes_ponder_waiter_without_stopping_search() {
        let control = Arc::new(SearchControl::new(true));
        let waiting = control.clone();
        let worker = std::thread::spawn(move || waiting.wait_for_release());
        control.ponderhit();
        worker.join().unwrap();
        assert!(!control.is_stopped());
        assert!(!control.is_pondering());
    }

    #[test]
    fn original_clock_budgets_are_preserved() {
        let tm = TimeManager::new(Limits::Fischer(120000, 1000), 1, 50);
        assert_eq!(tm.soft_bound, Duration::from_millis(2181));
        assert_eq!(tm.hard_bound, Duration::from_millis(88070));
        let tm = TimeManager::new(Limits::Time(1000), 1, 50);
        assert_eq!(tm.hard_bound, Duration::from_millis(985));
        assert!(tm.ponder.is_none());
    }
}
