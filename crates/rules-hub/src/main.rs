//! Placeholder of the program. The commands of `tasks-hub` (serve, start, stop, install ...) arrive
//! with the read-only UI phase; the hub they start is in the library and already tested.

fn main() {
    println!("{} {}", rules_hub::IDENTITY.binary, rules_hub::version());
}
