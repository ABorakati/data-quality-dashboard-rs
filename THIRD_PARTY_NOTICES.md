# Third-party notices

This project is licensed under the Apache License 2.0 (see [LICENSE](LICENSE) and [NOTICE](NOTICE)). The
material listed here comes from other authors and keeps its own licence. Where a file has a licence header, the
header is the authority. Copyright holders are taken from the headers. Where a header does not name one, this file
says "see file header" or "see font metadata" instead of guessing.

This file covers the viewer's browser assets and the licence policy for Rust dependencies. The OHDSI
DataQualityDashboard files that this port copies (see [resources/provenance.json](resources/provenance.json)) are
Apache-2.0, as stated in [NOTICE](NOTICE). The SqlRender tables are described in
[resources/sqlrender/NOTICE](resources/sqlrender/NOTICE).

## Viewer assets (`resources/viewer`)

| Component | Version | Files | Licence | Source | Copyright |
|---|---|---|---|---|---|
| jQuery | 3.7.1 | `resources/viewer/vendor/jquery/jquery.min.js` | MIT | <https://jquery.com> | OpenJS Foundation and other contributors (file header) |
| jQuery Easing | see file header (none stated) | `resources/viewer/vendor/jquery-easing/jquery.easing.min.js` | Not stated in the file. To be confirmed before release. | jQuery Easing project | see file header |
| Bootstrap | 4.3.1 | `resources/viewer/vendor/bootstrap/css/bootstrap.min.css`, `resources/viewer/vendor/bootstrap/js/bootstrap.min.js` | MIT | <https://getbootstrap.com> | The Bootstrap Authors; Twitter, Inc. (2011–2019, file header) |
| DataTables | 1.10.18 | `resources/viewer/vendor/dt/v/dt/dt-1.10.18/` | MIT | <https://datatables.net> | SpryMedia Ltd (the Buttons headers name SpryMedia; the DataTables header gives the licence link only) |
| DataTables Buttons | 1.5.6 | `resources/viewer/vendor/dt/buttons/1.5.6/` | MIT | <https://datatables.net> | SpryMedia Ltd (2016–2019, file headers) |
| FileSaver.js, inside `buttons.html5.min.js` | 1.3.3 | `resources/viewer/vendor/dt/buttons/1.5.6/js/buttons.html5.min.js` | MIT | <https://github.com/eligrey/FileSaver.js> | Copyright © 2016 Eli Grey (file header) |
| Font Awesome Free | 5.9.0 | `resources/viewer/vendor/fontawesome-free/` (CSS and `webfonts/`) | Icons: CC BY 4.0. Fonts: SIL OFL 1.1. Code: MIT. | <https://fontawesome.com> | see file header |
| Handlebars | Banner: 5.0.0-alpha.1. Embedded `VERSION` constant: 4.1.2. See open items. | `resources/viewer/htmlwidgets/lib/handlebars-latest.js` | MIT (Expat) | <https://handlebarsjs.com> | Copyright (C) 2011-2017 by Yehuda Katz (file header) |
| Start Bootstrap Resume | 5.0.7 | `resources/viewer/css/resume.css` (Start Bootstrap stylesheet with local customisations) | MIT | <https://startbootstrap.com> | Start Bootstrap, 2013–2019 (file header) |
| Muli (Google Fonts) | Font files `muli_v27` | `resources/viewer/vendor/googlefonts/muli_*.ttf`, `Muli-400,400i,800,800i.css` | SIL OFL 1.1 | <https://fonts.google.com> | see font metadata |
| Saira Extra Condensed (Google Fonts) | Font files `sairaec_v11` | `resources/viewer/vendor/googlefonts/sairaec_*.ttf`, `Saira_Extra_Condensed-500,700.css` | SIL OFL 1.1 | <https://fonts.google.com> | see font metadata |

### Notes on specific components

- **Font Awesome brand icons.** The glyphs in `fa-brands-400` are logos of other companies. They are trademarks of
  their owners and are not covered by the icon licence. The free icons are licensed as stated above, and the CC BY
  4.0 attribution is given in this file. The full licence is at <https://fontawesome.com/license/free>.
- **OHDSI logo.** `resources/viewer/img/ohdsi.png` is the OHDSI logo. It is a trademark of OHDSI and is not released under this
  project's licence. The viewer shows it to identify the upstream dashboard. This project is not affiliated with or
  endorsed by OHDSI.
- **Google Fonts.** The Muli and Saira Extra Condensed files are distributed under the SIL Open Font License,
  Version 1.1 (<https://openfontlicense.org>). The OFL asks that the licence accompany the font software when it is
  redistributed. See the open items below.
- **Port additions.** `resources/viewer/css/bridge.css`, `resources/viewer/js/bridge.js` and `resources/viewer/js/resume.js` are written for this port and follow the
  project licence.

## Rust dependencies

Rust crates are not vendored here. Their licences are checked by `cargo deny` against the allow-list in
[deny.toml](deny.toml), which runs in CI (`.github/workflows/security.yml`). At the time of writing, every crate in
`Cargo.lock` has a licence expression that the allow-list satisfies. The expressions are MIT, Apache-2.0, or
combinations of these with BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, Zlib, CC0-1.0 or CDLA-Permissive-2.0. For
the complete list, run:

```sh
cargo deny check licenses
```

or use `cargo about` to generate the full licence texts.

## MIT licence text

Several components above are MIT-licensed. The text below is the standard MIT licence. The copyright line for each
component is in the table above.

```text
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

Full licence texts: Font Awesome at <https://fontawesome.com/license/free>; CC BY 4.0 at
<https://creativecommons.org/licenses/by/4.0/legalcode>; SIL OFL 1.1 at <https://openfontlicense.org>.

## Open items before release

These items were found while writing this file. The owner should resolve them before the first public release.

1. **jQuery Easing.** The vendored file has no version, copyright or licence header. Confirm the version and licence
   against the upstream project, and add both to the table.
2. **Handlebars version.** The banner of `resources/viewer/htmlwidgets/lib/handlebars-latest.js` says 5.0.0-alpha.1, but its embedded
   `VERSION` constant is 4.1.2. Confirm which build was copied, and correct the table.
3. **OFL full text.** The SIL OFL 1.1 asks that its licence accompany the fonts. Either add the full OFL text to this
   file, or ship it next to the font files in the release archives.
4. **Font copyright holders.** The OFL font holders are in the font metadata, not in the CSS. Read them from the font
   files and add them to the table.
