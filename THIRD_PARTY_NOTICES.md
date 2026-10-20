# Third-Party Notices

This file describes the source dependencies and pinned executables distributed
with DCFC. Bundled executable notices are shipped alongside each executable;
this summary does not replace their full license files or transitive notices.

## Included in this repository

- `vendor/chatgpt-delegate`: based on `felixlark/chatgpt-delegate` commit
  `43f05ecab412fc40edd9ce9e00e4681bbd9139e7`, MIT. Preserve the
  copyright and license in `vendor/chatgpt-delegate/LICENSE`.
- React, React DOM, Tauri 2, the Tauri plugins, and their Rust/npm transitive
  dependencies are built into the desktop application. Exact versions are
  recorded in `package-lock.json` and `src-tauri/Cargo.lock`; their licenses
  must be collected for each release artifact, including the installer.
- The separately built Connector uses FastMCP, the MCP Python SDK, PyInstaller
  and their dependencies. The current build script uses version ranges rather
  than a complete Python lock file. A frozen dependency/notice inventory is
  required before distributing a Connector binary.

## Bundled executables

The NSIS/MSI installers bundle the following pinned, unmodified executables.
The build scripts verify archive and executable SHA-256 values. Installing
DCFC does not download these dependencies at first run.

- `mcp-proxy.exe`: `joshrotenberg/mcp-proxy` v0.4.3, distributed upstream with
  `LICENSE-APACHE` (Apache-2.0) and `LICENSE-MIT` (MIT). The locally tested exe
  has SHA-256
  `A1019FDA27715A984C1127DE90AC4398867984FFD8191C8CC9E4DA61B7D75CBC`
  and matches the official v0.4.3 Windows release archive exactly. Upstream
  maintains its releases; DCFC maintainers must validate version compatibility
  and document the supported version when DCFC is released.
- `tunnel-client.exe`: `openai/tunnel-client` v0.0.14, Apache-2.0 with upstream
  `LICENSE`, `NOTICE`, dependency license report and SPDX SBOM in its Windows
  release archive. The locally tested exe has SHA-256
  `FCC85A69EC0AD82518E4F8964F60C45E31787957782A0FC9C1B0C44E82D61B9B`
  and matches the official v0.0.14 archive exactly. Upstream maintains its
  releases and security updates; DCFC maintainers own integration testing and
  supported-version guidance. Its release archive also contains
  `cloudflared.exe`; DCFC does not include it. The Tunnel build extracts only
  `tunnel-client.exe`, `LICENSE`, `NOTICE`, the dependency license report and
  SPDX SBOM into `runtime/tunnel-client`. The archive SHA-256 is
  `784AB8DA7B5A88F0109F1FD8AAF0A1C86067430B896DDDF307EF7E3CC49FA1A5`.

MCP Proxy is installed at `runtime/mcp-proxy` with its Apache and MIT notices.
Release maintainers must review upstream changes and security updates, pin
new hashes explicitly, retain notices and run the managed-resource and
installed-payload tests before changing a supported dependency version.

The local Magic HTTP proxy is user-supplied software. DCFC does not endorse or
redistribute a particular proxy product.
