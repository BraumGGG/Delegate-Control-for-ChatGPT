# Third-Party Notices

This file describes the Gate A source and dependency boundary. It is not yet
the complete license archive for a public installer. The installer is not
approved for public release until a bill of materials and all required notices
are checked against its actual contents.

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

## External executables, not bundled by DCFC

The Gate A delivery model is user-supplied executable paths. The NSIS/MSI
bundle must not contain or download these programs:

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
  `cloudflared.exe`; do not copy it into DCFC's installer without a separate
  artifact/license audit.

Those licenses permit redistribution subject to their terms, but Gate A
deliberately does not redistribute either executable. Changing to a bundled
model requires a separately reviewed installer inventory, complete notices,
checksums, supply-chain update policy and installed-machine tests.

The local Magic HTTP proxy is user-supplied software. DCFC does not endorse or
redistribute a particular proxy product.
