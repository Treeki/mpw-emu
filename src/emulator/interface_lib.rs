use std::ffi::CString;
use crate::common::{four_cc, FourCC, OSErr};
use super::{EmuState, EmuUC, FuncResult, helpers::{ArgReader, UnicornExtras}};

const STDCLIB_ID: u32 = 100;

fn get_shared_library(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
    let (lib_name, arch_type, load_flags, conn_id, main_addr, err_message):
        (CString, FourCC, u32, u32, u32, u32) = reader.pstr().read6(uc)?;

    debug!(target: "InterfaceLib",
        "GetSharedLibrary(libName={lib_name:?}, archType={arch_type:?}, loadFlags={load_flags}, connID={conn_id:08X}, mainAddr={main_addr:08X}, errMessage={err_message:08X})");

    if arch_type == four_cc(*b"pwpc") && lib_name.as_bytes() == b"StdCLib" {
        uc.write_u32(conn_id, STDCLIB_ID)?;
        Ok(Some(0))
    } else {
        // not sure what the error code should be
        Ok(Some(OSErr::BadName.to_u32()))
    }
}

fn find_symbol(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
    let (conn_id, sym_name, sym_addr, sym_class):
        (u32, CString, u32, u32) = reader.pstr().read4(uc)?;

    debug!(target: "InterfaceLib",
        "FindSymbol(connID={conn_id:08X}, symName={sym_name:?}, symAddr={sym_addr:08X}, symClass={sym_class:08X})");

    if conn_id == STDCLIB_ID {
        // should probably do something with symClass...?
        let stub = state.find_stub(uc, "StdCLib", sym_name.to_str().unwrap())?;
        uc.write_u32(sym_addr, stub)?;
        debug!(target: "InterfaceLib", "returned stub: {stub:08X}");
        Ok(Some(0))
    } else {
        // not sure what the error code should be
        Ok(Some(OSErr::BadName.to_u32()))
    }
}

pub(super) fn install_shims(state: &mut EmuState) {
    state.install_shim_function("GetSharedLibrary", get_shared_library);
    state.install_shim_function("FindSymbol", find_symbol);
}
