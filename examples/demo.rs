//! Run it once and it listens; run it again with some words and the first copy prints them.
//!
//! `cargo run --example demo`, then, in another terminal, `cargo run --example demo -- one "two three"`.
use std::{env, thread, time::Duration};

use doorbell::Ring;

fn main() {
	let args: Vec<String> = env::args().skip(1).collect();
	match doorbell::ring("doorbell-demo", &args) {
		Ok(Ring::First(door)) => {
			println!("First copy, listening for 60 seconds.");
			door.listen(|message| println!("Another copy sent {message:?}"));
			thread::sleep(Duration::from_secs(60));
		}
		Ok(Ring::Delivered) => println!("Another copy is running, and has {args:?}."),
		Err(e) => eprintln!("Couldn't check for another copy: {e}"),
	}
}
