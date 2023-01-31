use crate::emulator::{EmuState, EmuUC, FuncResult};
use crate::emulator::helpers::ArgReader;
use crate::mac_roman;

fn write(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
    let (fildes, buf, count): (u32, u32, u32) = reader.read3(uc)?;
    let output = uc.mem_read_as_vec(buf.into(), count as usize)?;

    match state.stdio_files.get_mut(&fildes) {
        Some(f) => {
            if f.is_terminal() {
                Ok(Some(f.generic_write(&mac_roman::decode_buffer(&output, true))))
            } else {
                Ok(Some(f.generic_write(&output)))
            }
        }
        None => {
            warn!(target: "StdCLib", "write() is writing to invalid file {fildes:08X}");
            // set errno later?
            Ok(Some(0))
        }
    }
}

pub(super) fn install_shims(state: &mut EmuState) {
    state.install_shim_function("write", write);
}
