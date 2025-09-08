use crate::detours::ModuleExportResult;

use std::{fs, path::Path};

pub struct DllProxyBuilder {
    binary_name: String,
    binary_exports: Vec<ModuleExportResult>,
    pack_original_binary: bool,
}

fn generate_cc_header_from_stub(binary_name: &str) -> String {
    format!(
        r#"#pragma once

namespace {binary_name}
{{
  class {binary_name}_initialize
  {{
    public:
      static bool initialize();
  }};
}}"#
    )
}

fn generate_cc_dll_main_from_stub(binary_name: &str) -> String {
    format!(
        r#"#include "{binary_name}.h"

#include <windows.h>

BOOL WINAPI DllMain(_In_ HINSTANCE hinstDLL, _In_ DWORD fdwReason, _In_ LPVOID lpvReserved)
{{
  UNREFERENCED_PARAMETER(lpvReserved);

  switch (fdwReason)
  {{
    case DLL_PROCESS_ATTACH:
      DisableThreadLibraryCalls(hinstDLL);
      if ({binary_name}::{binary_name}_initialize::initialize()) {{}}
        break;

    case DLL_PROCESS_DETACH:
        break;
  }}
  return TRUE;
}}"#
    )
}

impl DllProxyBuilder {
    pub fn new(
        binary_name: String,
        binary_exports: Vec<ModuleExportResult>,
        pack_original_binary: bool,
    ) -> Self {
        let dll_proxy_builder = DllProxyBuilder {
            binary_name: binary_name,
            binary_exports: binary_exports,
            pack_original_binary: pack_original_binary,
        };

        dll_proxy_builder
    }

    pub fn binary_name(&mut self) -> &str {
        &self.binary_name
    }

    pub fn generate_cc_binary_header(&mut self, binary_path: &Path) -> String {
        let binary_bytes = fs::read(binary_path).unwrap();

        let mut res = String::new();
        res.push_str("#pragma once\n");
        res.push_str("#include <cstdint>\n\n");
        res.push_str(&format!("uint8_t {}_binary[] = {{", self.binary_name));

        res.push_str(
            &binary_bytes
                .iter()
                .map(|b| format!("0x{:02x}", b))
                .collect::<Vec<_>>()
                .join(", "),
        );
        res.push_str("};");

        res
    }

    pub fn generate_cc_header(&mut self) -> String {
        generate_cc_header_from_stub(&self.binary_name)
    }

    pub fn generate_cc_source(&mut self) -> String {
        let mut res = String::new();

        res.push_str(&format!("#include \"{}.h\"\n\n", self.binary_name));

        res.push_str("#include <windows.h>\n");
        res.push_str("#include <strsafe.h>\n");

        if self.pack_original_binary {
            res.push_str("\n#include <fstream>\n");
            res.push_str(&format!("#include \"{}_binary.h\"\n", self.binary_name));
        } else {
            res.push_str("#include <shlobj.h>\n");
        }

        res.push('\n');

        res.push_str(&format!(
            r#"namespace
{{
  const size_t {0}_size = {1};
  extern "C" FARPROC {0}_functions[{0}_size]; // prevents compiler mangling
  FARPROC {0}_functions[{0}_size];
}}
            "#,
            self.binary_name,
            self.binary_exports.len()
        ));

        res.push('\n');

        res.push_str(&format!(
            r#"namespace {0}
{{
  // static
  bool {0}_initialize::initialize()
  {{
    char {0}_path[MAX_PATH];
"#,
            self.binary_name
        ));

        if self.pack_original_binary {
            res.push_str(&format!(
                r#"
    if (!SUCCEEDED(StringCchPrintfA({0}_path, MAX_PATH, "%s%s", {0}_path, "{0}_data.bin")))
    {{
      return false;
    }}

    std::ofstream {0}_data_binary({0}_path, std::ios::binary | std::ios::trunc);
    {0}_data_binary.write(reinterpret_cast<const char*>({0}_binary), sizeof({0}_binary));
    {0}_data_binary.close();
"#,
                self.binary_name
            ));
        } else {
            res.push_str(&format!(
                r#"
    if (!SUCCEEDED(SHGetFolderPathA(0, CSIDL_SYSTEM, 0, 0, {0}_path)))
    {{
      return false;
    }}

    if (!SUCCEEDED(StringCchPrintfA({0}_path, MAX_PATH, "%s%s", {0}_path, "\\{0}.dll")))
    {{
      return false;
    }}
"#,
                self.binary_name
            ));
        }

        res.push_str(&format!(
            "\n    HMODULE {0}_module = LoadLibraryA({0}_path);\n",
            self.binary_name
        ));
        res.push_str(&format!("    if (!{}_module)\n    {{\n", self.binary_name));
        res.push_str("      return false;\n    }\n\n");

        for export in self.binary_exports.iter() {
            res.push_str(&format!(
                "    {}_functions[{}] = GetProcAddress({}_module, \"{}\");\n",
                self.binary_name,
                export.ordinal - 1,
                self.binary_name,
                export.name
            ));
        }

        res.push_str(&format!(
            r#"
    for (int i = 0; i < {0}_size; ++i)
    {{
        if (!{0}_functions[i])
        {{
            return false;
        }}
    }}

    return true;
"#,
            self.binary_name
        ));

        res.push_str("  }\n}\n");

        res
    }

    pub fn generate_cc_assembler(&mut self) -> String {
        let mut res: String = String::new();

        for export in self.binary_exports.iter() {
            res.push_str(&format!("PUBLIC {}_{}\n", self.binary_name, export.name));
        }

        res.push('\n');
        res.push_str(&format!("EXTERN {}_functions:QWORD\n", self.binary_name));
        res.push('\n');
        res.push_str(".code\n");
        res.push('\n');

        for export in self.binary_exports.iter() {
            res.push_str(&format!(
                "; [{:016X}] {}:{}\n",
                export.code as usize as u64, self.binary_name, export.ordinal
            ));
            res.push_str(&format!("{}_{} PROC\n", self.binary_name, export.name));
            res.push_str(&format!(
                "  jmp QWORD PTR [{}_functions + {}*8]\n",
                self.binary_name,
                export.ordinal - 1
            ));
            res.push_str(&format!("{}_{} ENDP\n", self.binary_name, export.name));
            res.push('\n');
        }

        res.push_str("END");

        res
    }

    pub fn generate_cc_dll_main(&mut self) -> String {
        generate_cc_dll_main_from_stub(&self.binary_name)
    }

    pub fn generate_cc_definitions(&mut self) -> String {
        let mut res = format!("LIBRARY \"{}\"\n\nEXPORTS\n", self.binary_name);
        for export in self.binary_exports.iter() {
            res.push_str(&format!(
                "  {}   = {}_{}   @{}   PRIVATE\n",
                export.name, self.binary_name, export.name, export.ordinal
            ));
        }
        res
    }
}
