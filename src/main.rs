fn main() {
    let buffer = std::env::args().skip(1).collect();
    kippers::run(buffer);
}
