use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, mpsc};

use crate::{
    board::{Board, NullBoardObserver},
    search::Report,
    thread::{SharedContext, Status, ThreadData},
    threadpool::ThreadPool,
    time::{Limits, SearchControl, TimeManager},
    tools,
    transposition::DEFAULT_TT_SIZE,
    types::{Color, Move, Piece, Score, Square, is_decisive, is_loss, is_win},
};

#[derive(Copy, Clone, PartialEq, Eq)]
enum Mode {
    Cli,
    Uci,
}

struct Settings {
    frc: bool,
    ponder: bool,
    move_overhead: u64,
    report: Report,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            frc: false,
            ponder: false,
            move_overhead: 50,
            report: Report::Full,
        }
    }
}

// The listener owns input, while message_loop owns the board and worker pool.
// Acknowledgements preserve input order without blocking stop/ponderhit during
// search. Native ponder support added in Kippers, October 15, 2026.
struct Command {
    text: String,
    accepted: Option<mpsc::SyncSender<()>>,
}

type ActiveSearch = Arc<Mutex<Option<Arc<SearchControl>>>>;

pub fn message_loop(mut buffer: VecDeque<String>) {
    let shared = Arc::new(SharedContext::default());
    let mut settings = Settings::default();
    let mut threads = ThreadPool::new(shared.clone());
    let mut board = Board::starting_position();

    let active_search: ActiveSearch = Arc::new(Mutex::new(None));
    let rx = spawn_listener(shared.clone(), active_search.clone());

    let mut mode = if buffer.is_empty() { Mode::Uci } else { Mode::Cli };

    loop {
        let Command { text: message, mut accepted } = if let Some(cmd) = buffer.pop_front() {
            Command { text: cmd, accepted: None }
        } else if mode == Mode::Uci {
            match rx.recv() {
                Ok(cmd) => cmd,
                Err(_) => break,
            }
        } else {
            break;
        };

        let tokens = message.split_whitespace().collect::<Vec<_>>();
        match tokens.as_slice() {
            ["uci"] => {
                uci();
                mode = Mode::Uci;
            }

            ["isready"] => println!("readyok"),

            ["go", tokens @ ..] => {
                go(&mut threads, &settings, &board, &shared, &active_search, tokens, accepted.take());
            }
            ["position", tokens @ ..] => position(&mut board, &settings, tokens),
            ["setoption", tokens @ ..] => set_option(&mut threads, &mut settings, &shared, tokens),
            ["ucinewgame"] => reset(&mut threads, &shared),

            ["stop"] => shared.status.set(Status::STOPPED),
            ["ponderhit"] => (), // Idle/CLI ponderhit has no effect.
            ["quit"] => {
                drop(threads);
                break;
            }

            // Non-UCI commands
            ["eval"] => eval(threads.main_thread(), &board),
            ["d"] => println!("{board}"),
            ["bench", args @ ..] => match mode {
                Mode::Uci => tools::bench::<true>(args),
                Mode::Cli => tools::bench::<false>(args),
            },
            ["perft", depth] => tools::perft(depth.parse().unwrap(), &mut board),
            ["perft"] => eprintln!("Usage: perft <depth>"),
            ["simpleperft", depth] => tools::simple_perft(depth.parse().unwrap(), &mut board),
            ["simpleperft"] => eprintln!("Usage: simpleperft <depth>"),
            ["islegalperft", depth] => tools::is_legal_perft(depth.parse().unwrap(), &mut board),
            ["islegalperft"] => eprintln!("Usage: islegalperft <depth>"),

            // Ignore empty lines
            [] => (),

            _ => eprintln!("Unknown command: '{}'", message.trim_end()),
        }

        if let Some(ack) = accepted {
            let _ = ack.send(());
        }

        // Auto-exit after last CLI command
        if matches!(mode, Mode::Cli) && buffer.is_empty() {
            drop(threads);
            break;
        }
    }
}

fn spawn_listener(shared: Arc<SharedContext>, active_search: ActiveSearch) -> mpsc::Receiver<Command> {
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        loop {
            let mut message = String::new();

            if !matches!(std::io::stdin().read_line(&mut message), Ok(n) if n > 0) {
                stop_search(&shared, &active_search);
                let _ = tx.send(Command { text: "quit".to_string(), accepted: None });
                break;
            }

            match message.trim() {
                "isready" => println!("readyok"),
                "stop" => stop_search(&shared, &active_search),
                "ponderhit" => {
                    if let Some(control) = active_search.lock().unwrap().as_ref() {
                        control.ponderhit();
                    }
                }
                "quit" => {
                    stop_search(&shared, &active_search);
                    let _ = tx.send(Command { text: "quit".to_string(), accepted: None });
                    break;
                }
                _ => {
                    // According to the UCI specs, commands that are unexpected
                    // in the current state should be ignored silently.
                    // (https://backscattering.de/chess/uci/#unexpected)
                    let busy = active_search.lock().unwrap().as_ref().is_some_and(|control| !control.is_stopped());
                    if !busy {
                        let (ack, received) = mpsc::sync_channel(0);
                        if tx.send(Command { text: message, accepted: Some(ack) }).is_err()
                            || received.recv().is_err()
                        {
                            break;
                        }
                    }
                }
            }
        }
    });

    rx
}

fn stop_search(shared: &SharedContext, active_search: &ActiveSearch) {
    let active = active_search.lock().unwrap();
    shared.status.set(Status::STOPPED);
    if let Some(control) = active.as_ref() {
        control.stop();
    }
}

fn uci() {
    println!("id name Kippers {}", env!("ENGINE_VERSION"));
    println!("id author Quedanomo");
    println!("option name Hash type spin default {DEFAULT_TT_SIZE} min 1 max 262144");
    println!("option name Threads type spin default 1 min 1 max {}", ThreadPool::available_threads());
    println!("option name MoveOverhead type spin default 50 min 0 max 2000");
    println!("option name Minimal type check default false");
    println!("option name Ponder type check default false");
    println!("option name Clear Hash type button");
    println!("option name UCI_Chess960 type check default false");

    println!("uciok");
}

fn reset(threads: &mut ThreadPool, shared: &Arc<SharedContext>) {
    threads.clear();
    shared.tt.clear(threads.len());

    shared.history.clear();
}

fn go(
    threads: &mut ThreadPool, settings: &Settings, board: &Board, shared: &Arc<SharedContext>,
    active_search: &ActiveSearch, tokens: &[&str], accepted: Option<mpsc::SyncSender<()>>,
) {
    let limits = parse_limits(board.side_to_move(), tokens);
    // go ponder is authoritative; the option advertises whether the GUI plans
    // to use pondering and enables the optional predicted-reply output.
    let pondering = tokens.contains(&"ponder");
    let control = Arc::new(SearchControl::new(pondering));
    let mut time_manager = TimeManager::new(limits, board.fullmove_number(), settings.move_overhead);
    if pondering {
        time_manager = time_manager.with_ponder(control.clone());
    }
    *active_search.lock().unwrap() = Some(control.clone());

    threads.execute_searches_with_start(time_manager, settings.report, board, shared, || {
        if let Some(ack) = accepted {
            let _ = ack.send(());
        }
    });

    // A depth/node/mate limit or a terminal position may finish the calculation
    // before the opponent moves. Never send bestmove while still pondering.
    control.wait_for_release();

    if threads[0].root_moves.is_empty() {
        *active_search.lock().unwrap() = None;
        println!("bestmove (none)");
        return;
    }

    let min_score = threads.iter().map(|v| v.root_moves[0].score).min().unwrap();
    let vote_value = |td: &ThreadData| (td.root_moves[0].score - min_score + 10) * td.completed_depth;

    let mut votes: HashMap<&Move, i32> = HashMap::new();
    for result in threads.iter() {
        *votes.entry(&result.root_moves[0].mv).or_default() += vote_value(result);
    }

    let mut best = 0;

    if !matches!(threads[best].time_manager.limits(), Limits::Depth(_)) {
        for current in 1..threads.len() {
            let is_better_candidate = || -> bool {
                let best = &threads[best];
                let current = &threads[current];

                if is_win(best.root_moves[0].score) {
                    return current.root_moves[0].score > best.root_moves[0].score;
                }

                if current.root_moves[0].score != -Score::INFINITE
                    && best.root_moves[0].score != -Score::INFINITE
                    && is_loss(best.root_moves[0].score)
                {
                    return current.root_moves[0].score < best.root_moves[0].score;
                }

                if current.root_moves[0].score != -Score::INFINITE && is_decisive(current.root_moves[0].score) {
                    return true;
                }

                let best_vote = votes[&best.root_moves[0].mv];
                let current_vote = votes[&current.root_moves[0].mv];

                !is_loss(current.root_moves[0].score)
                    && (current_vote > best_vote
                        || (current_vote == best_vote && vote_value(current) > vote_value(best)))
            };

            if is_better_candidate() {
                best = current;
            }
        }
    }

    if best != 0 {
        let depth = threads[best].completed_depth;
        threads[best].print_uci_info(depth);
    }

    let root = &threads[best].root_moves[0];
    let mut result = format!("bestmove {}", root.mv.to_uci(board));
    if settings.ponder
        && let Some(&reply) = root.pv.line().first()
    {
        let mut next = board.clone();
        next.make_move(root.mv, &mut NullBoardObserver);
        if next.generate_all_moves().iter().any(|entry| entry.mv == reply) {
            result.push_str(&format!(" ponder {}", reply.to_uci(&next)));
        }
    }

    *active_search.lock().unwrap() = None;
    println!("{result}");
}

fn position(board: &mut Board, settings: &Settings, mut tokens: &[&str]) {
    while !tokens.is_empty() {
        match tokens {
            ["startpos", rest @ ..] => {
                *board = Board::starting_position();
                tokens = rest;
            }
            ["fen", rest @ ..] => {
                match Board::from_fen(&rest.join(" ")) {
                    Ok(b) => *board = b,
                    Err(e) => eprintln!("Invalid FEN: {e:?}"),
                }
                board.set_frc(settings.frc);
                tokens = rest;
            }
            ["moves", rest @ ..] => {
                for uci_move in rest {
                    make_uci_move(board, uci_move);
                }
                break;
            }
            _ => tokens = &tokens[1..],
        }
    }
}

fn make_uci_move(board: &mut Board, uci_move: &str) {
    let moves = board.generate_all_moves();
    if let Some(mv) = moves.iter().map(|entry| entry.mv).find(|mv| mv.to_uci(board) == uci_move) {
        board.make_move(mv, &mut NullBoardObserver);
    }
}

fn set_option(threads: &mut ThreadPool, settings: &mut Settings, shared: &Arc<SharedContext>, tokens: &[&str]) {
    match tokens {
        ["name", "Ponder", "value", v] => match *v {
            "true" => settings.ponder = true,
            "false" => settings.ponder = false,
            _ => eprintln!("Invalid value: '{v}'"),
        },
        ["name", "Minimal", "value", v] => match *v {
            "true" => settings.report = Report::Minimal,
            "false" => settings.report = Report::Full,
            _ => eprintln!("Invalid value: '{v}'"),
        },
        ["name", "Clear", "Hash"] => {
            shared.tt.clear(threads.len());
            println!("info string Hash cleared");
        }
        ["name", "Hash", "value", v] => {
            shared.tt.resize(threads.len(), v.parse().unwrap());
            println!("info string set Hash to {v} MB");
        }
        ["name", "Threads", "value", v] => {
            threads.set_count(v.parse().unwrap_or(1));
            println!("info string set Threads to {}", threads.len());
        }
        ["name", "MoveOverhead", "value", v] => {
            settings.move_overhead = v.parse().unwrap();
            println!("info string set MoveOverhead to {v} ms");
        }
        ["name", "UCI_Chess960", "value", v] => {
            settings.frc = v.parse().unwrap_or_default();
            println!("info string set UCI_Chess960 to {v}");
        }
        _ => eprintln!("Unknown option: '{}'", tokens.join(" ").trim_end()),
    }
}

fn eval(td: &mut ThreadData, board: &Board) {
    td.nnue.full_refresh(board);
    td.nnue.evaluate(board);

    let side = board.side_to_move();

    println!("NNUE derived piece values:");
    println!("+-------+-------+-------+-------+-------+-------+-------+-------+");
    for rank in (0..8).rev() {
        print!("|");
        for file in 0..8 {
            let sq = Square::from_rank_file(rank, file);
            let piece = board.piece_on(sq);
            let piece_str = if piece == Piece::None { " ".to_string() } else { piece.to_string() };
            print!("  {piece_str:^3}  |");
        }
        println!();

        print!("|");
        for file in 0..8 {
            let sq = Square::from_rank_file(rank, file);
            match td.nnue.piece_contribution(board, sq) {
                None => print!("       |"),
                Some(v) => {
                    let white_relative = if side == Color::White { v } else { -v };
                    let val = white_relative as f32 / 100.0;
                    print!("{val:+6.2} |");
                }
            }
        }
        println!();
        println!("+-------+-------+-------+-------+-------+-------+-------+-------+");
    }

    let used_bucket = crate::nnue::OUTPUT_BUCKETS_LAYOUT[board.occupancies().popcount()];

    println!("\nNNUE output buckets (White's POV):");
    println!("+-------------+------------+");
    println!("|   Buckets   |   Total    |");
    println!("+-------------+------------+");

    for bucket in 0..8 {
        let raw_score = td.nnue.eval_with_bucket(board, bucket);
        let white_score = if side == Color::White { raw_score } else { -raw_score };
        let total = white_score as f32 / 100.0;

        if bucket == used_bucket {
            println!("|  >   {bucket:<7}| {total:+7.2}    |");
        } else {
            println!("|{bucket:^13}| {total:+7.2}    |");
        }
    }
    println!("+-------------+------------+");

    let final_eval = td.nnue.evaluate(board);
    let final_total = (if side == Color::White { final_eval } else { -final_eval }) as f32 / 100.0;
    println!("\nNNUE evaluation        {final_total:+.2} (White's POV)");
}

fn parse_limits(color: Color, tokens: &[&str]) -> Limits {
    if tokens.contains(&"infinite") {
        return Limits::Infinite;
    }

    let mut main = None;
    let mut inc = None;
    let mut moves = None;

    // Flags such as ponder do not have a value and must not shift the pairing
    // of subsequent clock arguments. Also accept flags after limit arguments.
    for (index, &name) in tokens.iter().enumerate() {
        if let Some(value) = tokens.get(index + 1).and_then(|value| value.parse::<u64>().ok()) {

            match name {
                "depth" if value > 0 => return Limits::Depth(value as i32),
                "movetime" if value > 0 => return Limits::Time(value),
                "nodes" if value > 0 => return Limits::Nodes(value),
                "mate" if value > 0 => return Limits::Mate(value),

                "wtime" if Color::White == color => main = Some(value),
                "btime" if Color::Black == color => main = Some(value),
                "winc" if Color::White == color => inc = Some(value),
                "binc" if Color::Black == color => inc = Some(value),
                "movestogo" => moves = Some(value),

                _ => continue,
            }
        }
    }

    if main.is_none() && inc.is_none() {
        return Limits::Infinite;
    }

    let main = main.unwrap_or_default();
    let inc = inc.unwrap_or_default();

    match moves {
        Some(moves) => Limits::Cyclic(main, inc, moves),
        None => Limits::Fischer(main, inc),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ponder_flag_does_not_displace_clock_arguments() {
        for tokens in [
            vec!["ponder", "wtime", "120000", "btime", "60000", "winc", "1000", "binc", "2000"],
            vec!["wtime", "120000", "ponder", "btime", "60000", "winc", "1000", "binc", "2000"],
            vec!["wtime", "120000", "btime", "60000", "winc", "1000", "binc", "2000", "ponder"],
        ] {
            assert_eq!(parse_limits(Color::White, &tokens), Limits::Fischer(120000, 1000));
            assert_eq!(parse_limits(Color::Black, &tokens), Limits::Fischer(60000, 2000));
        }
        assert_eq!(parse_limits(Color::White, &["ponder", "movetime", "250"]), Limits::Time(250));
        assert_eq!(parse_limits(Color::White, &["ponder", "depth", "4"]), Limits::Depth(4));
        assert_eq!(parse_limits(Color::White, &["nodes", "100", "ponder"]), Limits::Nodes(100));
        assert_eq!(parse_limits(Color::White, &["ponder", "mate", "1"]), Limits::Mate(1));
        assert_eq!(parse_limits(Color::White, &["ponder", "infinite"]), Limits::Infinite);
        assert_eq!(
            parse_limits(Color::White, &["ponder", "wtime", "5000", "winc", "20", "movestogo", "10"]),
            Limits::Cyclic(5000, 20, 10),
        );
    }

    fn test_position_helper(tokens: &[&str]) -> Board {
        let settings = Settings::default();
        let mut board = Board::starting_position();

        position(&mut board, &settings, tokens);
        board.clone()
    }

    #[test]
    fn test_position_startpos() {
        let board = test_position_helper(&["startpos"]);
        assert_eq!(board.to_fen(), "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        let board = test_position_helper(&[]);
        assert_eq!(board.to_fen(), "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    }

    #[test]
    fn test_position_startpos_multiple_moves() {
        let board = test_position_helper(&["moves", "e2e4", "e7e5", "g1f3"]);
        assert_eq!(board.side_to_move(), Color::Black);
        let fen = board.to_fen();
        let fen_position = fen.split_whitespace().next().unwrap();
        assert!(fen_position.contains("5N2"));
    }

    #[test]
    fn test_position_fen_with_moves() {
        let board = test_position_helper(&[
            "fen",
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR",
            "b",
            "KQkq",
            "e3",
            "0",
            "1",
            "moves",
            "e7e5",
        ]);
        assert_eq!(board.side_to_move(), Color::White);
    }

    #[test]
    fn test_position_empty_moves_list() {
        let board = test_position_helper(&["moves"]);
        assert_eq!(board.to_fen(), "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    }

    #[test]
    fn test_position_invalid_move_ignored() {
        let board = test_position_helper(&["moves", "e2e4", "invalid", "e7e5"]);
        assert_eq!(board.side_to_move(), Color::White);
    }

    #[test]
    fn test_position_long_move_sequence() {
        let board = test_position_helper(&["moves", "e2e4", "e7e5", "g1f3", "b8c6", "f1b5", "a7a6"]);
        assert_eq!(board.side_to_move(), Color::White);
    }

    #[test]
    fn test_position_castling() {
        let board = test_position_helper(&[
            "fen",
            "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R",
            "w",
            "KQkq",
            "-",
            "0",
            "1",
            "moves",
            "e1g1",
        ]);
        assert_eq!(board.side_to_move(), Color::Black);
    }

    #[test]
    fn test_position_en_passant() {
        let board = test_position_helper(&[
            "fen",
            "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR",
            "w",
            "KQkq",
            "f6",
            "0",
            "1",
            "moves",
            "e5f6",
        ]);
        assert_eq!(board.side_to_move(), Color::Black);
    }

    #[test]
    fn test_position_promotion() {
        let board = test_position_helper(&["fen", "8/P7/8/8/8/8/8/4K2k", "w", "-", "-", "0", "1", "moves", "a7a8q"]);
        assert_eq!(board.side_to_move(), Color::Black);
    }

    #[test]
    fn test_make_uci_move_invalid() {
        let mut board = Board::starting_position();
        let fen_before = board.to_fen();
        make_uci_move(&mut board, "invalid_move");
        assert_eq!(board.to_fen(), fen_before);
    }

    #[test]
    fn test_position_moves_without_startpos_ignored() {
        let board = test_position_helper(&["moves", "e2e4", "e7e5"]);
        assert_eq!(board.to_fen(), "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2");
    }
}
