use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use unicorn_engine::{Unicorn, RegisterPPC};
use unicorn_engine::unicorn_const::{Arch, Mode, Permission};

use crate::common::FourCC;
use crate::filesystem::FileSystem;
use crate::linker;
use crate::pef;
use crate::resources::Resources;

mod c_ctype;
mod c_stdio;
mod c_stdlib;
mod c_string;
mod heap;
mod helpers;
mod mac_files;
mod mac_gestalt;
mod mac_low_mem;
mod mac_memory;
mod mac_os_utils;
mod mac_quickdraw;
mod mac_resources;
mod mac_text_utils;

type UcResult<T> = Result<T, unicorn_engine::unicorn_const::uc_error>;

type LibraryShim = fn(&mut EmuUC, &mut EmuState, &mut helpers::ArgReader) -> UcResult<Option<u32>>;

struct ShimSymbol {
	shim_address: u32,
	class: pef::SymbolClass,
	name: String,
	func: Option<LibraryShim>
}

struct EmuState {
	start_time: Instant,
	imports: Vec<ShimSymbol>,
	dummy_cursor_handle: Option<u32>,
	resources: Rc<Resources>,
	loaded_resources: HashMap<(FourCC, i16), u32>,
	env_var_map: HashMap<String, u32>,
	strtok_state: u32,
	stdio_files: HashMap<u32, c_stdio::CFile>,
	file_handles: HashMap<u16, std::fs::File>,
	next_file_handle: u16,
	exit_status: Option<i32>,
	heap: heap::Heap,
	filesystem: FileSystem
}

impl EmuState {
	fn new(exe: &linker::Executable, resources: Rc<Resources>) -> Self {
		let mut state = EmuState {
			start_time: Instant::now(),
			imports: Vec::new(),
			dummy_cursor_handle: None,
			resources,
			loaded_resources: HashMap::new(),
			env_var_map: HashMap::new(),
			strtok_state: 0,
			stdio_files: HashMap::new(),
			file_handles: HashMap::new(),
			next_file_handle: 1,
			exit_status: None,
			heap: heap::Heap::new(0x30000000, 1024 * 1024 * 8, 512),
			filesystem: FileSystem::new()
		};

		for (import, shim_address) in exe.imports.iter().zip(&exe.shim_addrs) {
			if import.class == pef::SymbolClass::Data {
				trace!(target: "emulator", "(!) Data import: {}", import.name);
			}

			state.imports.push(ShimSymbol {
				shim_address: *shim_address,
				class: import.class,
				name: import.name.clone(),
				func: None
			});
		}

		state
	}

	fn get_shim_addr(&self, name: &str) -> Option<u32> {
		for import in &self.imports {
			if import.name == name {
				return Some(import.shim_address);
			}
		}
		None
	}

	fn install_shim_function(&mut self, name: &str, func: LibraryShim) {
		for import in &mut self.imports {
			if import.name == name {
				import.func = Some(func);
			}
		}
	}
}

type EmuUC<'a> = Unicorn<'a, Rc<RefCell<EmuState>>>;

fn code_hook(_uc: &mut EmuUC, _addr: u64, _size: u32) {
}

fn intr_hook(uc: &mut EmuUC, _number: u32) {
	let rtoc = uc.reg_read(RegisterPPC::GPR2).unwrap();
	let lr = uc.reg_read(74).unwrap();
	let pc = uc.pc_read().unwrap();
	
	let state = Rc::clone(uc.get_data());
	let mut state = state.borrow_mut();

	if state.exit_status.is_some() {
		// we have exited, go away
		// (unicorn keeps running code afterwards)
		uc.emu_stop().unwrap();
		return;
	}

	match state.imports[rtoc as usize].func {
		Some(func) => {
			let mut arg_reader = helpers::ArgReader::new();
			match func(uc, &mut state, &mut arg_reader) {
				Ok(Some(result)) => uc.reg_write(RegisterPPC::GPR3, result.into()).unwrap(),
				Ok(None) => {},
				Err(e) => {
					error!(target: "emulator", "Error {e:?} while executing {} (lr={lr:08x})", state.imports[rtoc as usize].name);
				}
			}
		}
		None => {
			warn!(target: "emulator", "Unimplemented call to {} @{lr:08X}", state.imports[rtoc as usize].name);
		}
	}

	// NOTE: next unicorn will not need this i think?
	uc.set_pc(pc + 4).unwrap();
}


type FuncResult = UcResult<Option<u32>>;

fn dump_context(uc: &EmuUC) {
	println!("  PC: {:08x} / LR: {:08x}", uc.pc_read().unwrap(), uc.reg_read(74).unwrap());
	println!("  R00: {:08x} / R08: {:08x} / R16: {:08x} / R24: {:08x}", uc.reg_read(RegisterPPC::GPR0).unwrap(), uc.reg_read(RegisterPPC::GPR8).unwrap(), uc.reg_read(RegisterPPC::GPR16).unwrap(), uc.reg_read(RegisterPPC::GPR24).unwrap());
	println!("  R01: {:08x} / R09: {:08x} / R17: {:08x} / R25: {:08x}", uc.reg_read(RegisterPPC::GPR1).unwrap(), uc.reg_read(RegisterPPC::GPR9).unwrap(), uc.reg_read(RegisterPPC::GPR17).unwrap(), uc.reg_read(RegisterPPC::GPR25).unwrap());
	println!("  R02: {:08x} / R10: {:08x} / R18: {:08x} / R26: {:08x}", uc.reg_read(RegisterPPC::GPR2).unwrap(), uc.reg_read(RegisterPPC::GPR10).unwrap(), uc.reg_read(RegisterPPC::GPR18).unwrap(), uc.reg_read(RegisterPPC::GPR26).unwrap());
	println!("  R03: {:08x} / R11: {:08x} / R19: {:08x} / R27: {:08x}", uc.reg_read(RegisterPPC::GPR3).unwrap(), uc.reg_read(RegisterPPC::GPR11).unwrap(), uc.reg_read(RegisterPPC::GPR19).unwrap(), uc.reg_read(RegisterPPC::GPR27).unwrap());
	println!("  R04: {:08x} / R12: {:08x} / R20: {:08x} / R28: {:08x}", uc.reg_read(RegisterPPC::GPR4).unwrap(), uc.reg_read(RegisterPPC::GPR12).unwrap(), uc.reg_read(RegisterPPC::GPR20).unwrap(), uc.reg_read(RegisterPPC::GPR28).unwrap());
	println!("  R05: {:08x} / R13: {:08x} / R21: {:08x} / R29: {:08x}", uc.reg_read(RegisterPPC::GPR5).unwrap(), uc.reg_read(RegisterPPC::GPR13).unwrap(), uc.reg_read(RegisterPPC::GPR21).unwrap(), uc.reg_read(RegisterPPC::GPR29).unwrap());
	println!("  R06: {:08x} / R14: {:08x} / R22: {:08x} / R30: {:08x}", uc.reg_read(RegisterPPC::GPR6).unwrap(), uc.reg_read(RegisterPPC::GPR14).unwrap(), uc.reg_read(RegisterPPC::GPR22).unwrap(), uc.reg_read(RegisterPPC::GPR30).unwrap());
	println!("  R07: {:08x} / R15: {:08x} / R23: {:08x} / R31: {:08x}", uc.reg_read(RegisterPPC::GPR7).unwrap(), uc.reg_read(RegisterPPC::GPR15).unwrap(), uc.reg_read(RegisterPPC::GPR23).unwrap(), uc.reg_read(RegisterPPC::GPR31).unwrap());
}

pub fn emulate(exe: &linker::Executable, resources: Rc<Resources>, args: &[String], env_vars: &[(String, String)]) -> UcResult<i32> {
	let state = Rc::new(RefCell::new(EmuState::new(exe, resources)));
	let mut uc = Unicorn::new_with_data(Arch::PPC, Mode::BIG_ENDIAN | Mode::PPC32, Rc::clone(&state))?;

	uc.mem_map(exe.memory_base as u64, (exe.memory.len() + 0x3FFF) & !0x3FFF, Permission::ALL)?;
	uc.mem_write(exe.memory_base as u64, &exe.memory)?;

	// enable floating point
	uc.reg_write(77, uc.reg_read(77)? | (1 << 13))?;

	// set up the stack
	uc.reg_write(RegisterPPC::GPR1, (exe.stack_addr + exe.stack_size - 0x20).into())?;

	// uc.add_code_hook(0, 0xFFFFFFFF, code_hook)?;
	uc.add_intr_hook(intr_hook)?;

	let exec_end_address = exe.memory_end_addr();

	{
		let mut state = state.borrow_mut();

		state.heap.init(&mut uc)?;

		// populate IntEnv
		c_stdlib::setup_environment(&mut uc, &mut state, args, env_vars)?;

		// inject shim functions
		c_ctype::install_shims(&mut state);
		c_stdio::install_shims(&mut state);
		c_stdlib::install_shims(&mut state);
		c_string::install_shims(&mut state);
		mac_files::install_shims(&mut state);
		mac_gestalt::install_shims(&mut state);
		mac_low_mem::install_shims(&mut state);
		mac_memory::install_shims(&mut state);
		mac_os_utils::install_shims(&mut state);
		mac_quickdraw::install_shims(&mut state);
		mac_resources::install_shims(&mut state);
		mac_text_utils::install_shims(&mut state);

		for symbol in &state.imports {
			if symbol.func.is_none() && symbol.class == pef::SymbolClass::TVect {
				warn!(target: "emulator", "Executable imports unimplemented function: {}", symbol.name);
			}
		}
	}

	if exe.init_vector > 0 {
		// TODO: C++ binaries will probably need this
		let code = exe.get_u32(exe.init_vector);
		let rtoc = exe.get_u32(exe.init_vector + 4);
		warn!(target: "emulator", "Init: code={:08X}, rtoc={:08x}", code, rtoc);
		warn!(target: "emulator", " !!! Not implemented !!!");
	}

	if exe.main_vector > 0 {
		let code = exe.get_u32(exe.main_vector);
		let rtoc = exe.get_u32(exe.main_vector + 4);
		debug!(target: "emulator", "Main: code={:08X}, rtoc={:08x}", code, rtoc);

		uc.reg_write(RegisterPPC::GPR2, rtoc.into())?;
		uc.reg_write(74, exec_end_address.into())?; // LR

		if let Err(e) = uc.emu_start(code.into(), exec_end_address.into(), 0, 0) {
			let state = state.borrow();
			if state.exit_status.is_none() {
				error!(target: "emulator", "Main execution failed: {:?}", e);
				dump_context(&uc);
				return Err(e);
			}
		}
	}

	let exit_status = state.borrow().exit_status.unwrap_or(0);
	Ok(exit_status)
}
