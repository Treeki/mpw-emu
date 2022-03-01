#[macro_use]
extern crate log;

use std::rc::Rc;

mod common;
mod emulator;
mod filesystem;
mod linker;
mod macbinary;
mod mac_roman;
mod pef;
mod resources;

fn main() {
	env_logger::init();

	let env_vars = std::env::vars().collect::<Vec<_>>();
	let args = std::env::args().skip(1).collect::<Vec<_>>();

	let exe_bytes = match std::fs::read(&args[0]) {
		Ok(b) => b,
		Err(e) => {
			eprintln!("Cannot read executable: {:?}", args[0]);
			eprintln!("{}", e);
			return;
		}
	};

	let mb = macbinary::extract_file(&exe_bytes).expect("MacBinary unpacking failed");
	let res = resources::parse_resources(&mb.resource).expect("Resource fork loading failed");
	let pef = pef::read_pef(&mb.data).expect("PEF parsing failed");

	let mut exe = linker::Executable::new();
	exe.load_pef(pef);

	let code = emulator::emulate(&exe, Rc::new(res), &args, &env_vars).unwrap();
	std::process::exit(code);
}
