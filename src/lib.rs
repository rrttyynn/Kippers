#![allow(unsafe_op_in_unsafe_fn)]
#![warn(clippy::large_types_passed_by_value)]
#![warn(clippy::trivially_copy_pass_by_ref)]
#![warn(clippy::redundant_clone)]

mod board;
mod evaluation;
mod history;
mod lookup;
mod movepick;
mod nnue;
mod search;
mod setwise;
mod stack;
mod thread;
mod threadpool;
mod time;
mod transposition;
mod types;

mod tools;

mod uci;

pub fn run(buffer: std::collections::VecDeque<String>) {
    lookup::initialize();
    nnue::initialize();
    uci::message_loop(buffer);
}
