use std::ffi::CString;
use crate::emulator::{EmuState, EmuUC, FuncResult};
use crate::emulator::helpers::{ArgReader, UnicornExtras};

pub(super) struct Checkout {
    feature: CString,
    version: CString
}

fn flex_init(_uc: &mut EmuUC, _state: &mut EmuState, _reader: &mut ArgReader) -> FuncResult {
    debug!(target: "FlexLM", "Flex_Init()");
    Ok(None)
}

fn lp_checkout(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
    let (code, policy, feature, version, num_lic, path, lp):
        (u32, u32, CString, CString, u32, CString, u32) = reader.read7(uc)?;

    debug!(target: "FlexLM", "lp_checkout(code={code:08X}, policy={policy:X}, feature={feature:?}, version={version:?}, num_lic={num_lic}, path={path:?}, lp={lp:08X})");
    uc.write_u32(lp, state.next_checkout)?;
    state.checkouts.insert(state.next_checkout, Checkout { feature, version });
    state.next_checkout += 0x100;

    Ok(Some(0))
}

fn lp_checkin(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
    let lp: u32 = reader.read1(uc)?;

    debug!(target: "FlexLM", "lp_checkin(lp={lp:08X})");
    if let Some(checkout) = state.checkouts.remove(&lp) {
        let feature = checkout.feature;
        let version = checkout.version;
        debug!(target: "FlexLM", "feature={feature:?}, version={version:?}");
    }

    Ok(None)
}

pub(super) fn install_shims(state: &mut EmuState) {
    state.install_shim_function("Flex_Init", flex_init);
    state.install_shim_function("lp_checkout", lp_checkout);
    state.install_shim_function("lp_checkin", lp_checkin);
}
