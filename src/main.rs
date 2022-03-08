#![feature(int_roundings)]
#[macro_use]
extern crate log;

use std::rc::Rc;

mod common;
mod emulator;
mod linker;
mod macbinary;
mod mac_roman;
mod filesystem;
mod pef;
mod resources;

fn main() {
	env_logger::init();

	let env_vars = std::env::vars().collect::<Vec<_>>();
	let args = std::env::args().skip(1).collect::<Vec<_>>();
	if args.is_empty() {
		eprintln!("No executable specified");
		return;
	}

	let file = match filesystem::MacFile::open(&args[0]) {
		Ok(f) => f,
		Err(e) => {
			eprintln!("Cannot read executable: {:?}", args[0]);
			eprintln!("{}", e);
			return;
		}
	};
	let res = resources::parse_resources(&file.resource_fork).expect("Resource fork loading failed");
	let pef = pef::read_pef(&file.data_fork).expect("PEF parsing failed");

	let mut exe = linker::Executable::new();
	exe.load_pef(pef);

	let code = emulator::emulate(&exe, Rc::new(res), &args, &env_vars).unwrap();
	std::process::exit(code);
}
