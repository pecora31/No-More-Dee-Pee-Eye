# Third-party components

- **Inter** — https://github.com/google/fonts/tree/main/ofl/inter — SIL Open Font License 1.1. Embedded unchanged; license in `ui/fonts/INTER-OFL.txt` (portable: `FONT-LICENSE.txt`).

No More Dee Pee Eye application source is distributed under GPL-3.0-only. Dependencies retain their respective licenses; this project does not claim ownership of Zapret2, WinDivert, Cygwin or Slint.

- **Slint 1.14.1** — https://github.com/slint-ui/slint/tree/v1.14.1 — selected GPL-3.0-only option. Other upstream licensing options are not selected here.
- **Zapret2 / winws2** — https://github.com/bol-van/zapret2 — bundled without modification from the pinned official Windows bundle. Review upstream source file notices and bundled notices before redistributing outside a personal development build.
- **Official Windows bundle** — https://github.com/bol-van/zapret-win-bundle/tree/6eb463a6758fb48cd101bc55dfd057e6e9d98af1 — binary provenance and complete distribution context.
- **WinDivert** — https://github.com/basil00/WinDivert — LGPLv3 or GPLv2; see upstream LICENSE and documentation. The bundled driver is upstream-signed.
- **Cygwin** — https://cygwin.com/licensing.html — GPLv3+ with the upstream linking exception; redistributors must meet the applicable source-distribution obligations.
- Rust dependency versions are fixed in `Cargo.lock`. Each crate's package metadata and license files are the authority for its terms.

The local portable artifact is a development build. Public distribution requires a complete license/source fulfillment package for all included binaries, including the exact corresponding Cygwin/WinDivert sources. A URL alone is not asserted to satisfy those obligations.
