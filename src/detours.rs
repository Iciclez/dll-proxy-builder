use std::ffi::CStr;
use std::os::raw::{c_char, c_ulong, c_void};
use windows_sys::Win32::Foundation::HMODULE;

#[derive(Debug, Clone)]
pub struct ModuleExportResult {
    pub name: String,
    pub ordinal: u32,
    pub code: *mut std::ffi::c_void,
}

pub type PfDetourEnumerateExportCallback = Option<
    unsafe extern "system" fn(
        context: *mut c_void,
        ordinal: c_ulong,
        name: *const c_char,
        code: *mut c_void,
    ) -> i32,
>;

#[link(name = "detours64", kind = "static")]
unsafe extern "system" {
    pub fn DetourEnumerateExports(
        hModule: HMODULE,
        pContext: *mut c_void,
        pfExport: PfDetourEnumerateExportCallback,
    ) -> i32;
}

pub unsafe extern "system" fn detour_enumerate_export_callback(
    ctx: *mut std::ffi::c_void,
    ordinal: u32,
    name: *const std::os::raw::c_char,
    code: *mut std::ffi::c_void,
) -> i32 {
    unsafe {
        let exports = &mut *(ctx as *mut Vec<ModuleExportResult>);
        let symbol = if !name.is_null() {
            CStr::from_ptr(name).to_string_lossy().into_owned()
        } else {
            "[NONAME]".to_string()
        };
        exports.push(ModuleExportResult {
            ordinal,
            name: symbol,
            code,
        });

        1 // RETURN TRUE
    }
}
