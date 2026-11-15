# Third-party notices

This project is licensed under the Apache License 2.0 (see [LICENSE](LICENSE) and [NOTICE](NOTICE)). The
material listed here comes from other authors and keeps its own licence. Where a file has a licence header, the
header is the authority. Copyright holders come from the file headers, from the metadata of the font files, or, for
files with no header, from the LICENSE of the official package that the file was checked against byte for byte.

This file covers the viewer's browser assets and the licence policy for Rust dependencies. The OHDSI
DataQualityDashboard files that this port copies (see [resources/provenance.json](resources/provenance.json)) are
Apache-2.0, as stated in [NOTICE](NOTICE). The SqlRender tables are described in
[crates/sqlrender/data/NOTICE](crates/sqlrender/data/NOTICE).

## Viewer assets (`resources/viewer`)

| Component | Version | Files | Licence | Source | Copyright |
|---|---|---|---|---|---|
| jQuery | 3.7.1 | `resources/viewer/vendor/jquery/jquery.min.js` | MIT | <https://jquery.com> | OpenJS Foundation and other contributors (file header) |
| jQuery Easing | 1.4.1 (identical, byte for byte, to `jquery.easing@1.4.1` on npm) | `resources/viewer/vendor/jquery-easing/jquery.easing.min.js` | BSD-3-Clause, two notices: the plugin and the easing equations. Full text below. | <https://github.com/gdsmith/jquery.easing> | © 2008 George McGinley Smith (the plugin); © 2001 Robert Penner (the easing equations). The minified file has no header: both come from the package's `LICENSE` |
| Bootstrap | 4.3.1 | `resources/viewer/vendor/bootstrap/css/bootstrap.min.css`, `resources/viewer/vendor/bootstrap/js/bootstrap.min.js` | MIT | <https://getbootstrap.com> | The Bootstrap Authors; Twitter, Inc. (2011–2019, file header) |
| DataTables | 1.10.18 | `resources/viewer/vendor/dt/v/dt/dt-1.10.18/` | MIT | <https://datatables.net> | SpryMedia Ltd (the Buttons headers name SpryMedia; the DataTables header gives the licence link only) |
| DataTables Buttons | 1.5.6 | `resources/viewer/vendor/dt/buttons/1.5.6/` | MIT | <https://datatables.net> | SpryMedia Ltd (2016–2019, file headers) |
| FileSaver.js, inside `buttons.html5.min.js` | 1.3.3 | `resources/viewer/vendor/dt/buttons/1.5.6/js/buttons.html5.min.js` | MIT | <https://github.com/eligrey/FileSaver.js> | Copyright © 2016 Eli Grey (file header) |
| Font Awesome Free | 5.9.0 (the three `fa-brands-400` font files are identical, byte for byte, to `@fortawesome/fontawesome-free@5.9.0` on npm; `all.min.css` differs only in the line endings of its header) | `resources/viewer/vendor/fontawesome-free/` (`css/all.min.css` and `webfonts/fa-brands-400.*`) | Fonts: SIL OFL 1.1 (full text below). CSS: MIT. The CC BY 4.0 icon licence covers the SVG and JS packaging, which is not included here. | <https://fontawesome.com> | Fonts: "Copyright (c) Font Awesome" (font metadata). CSS: see the header of `all.min.css` |
| Handlebars | A development build from Handlebars' master branch, not a release. Banner: 5.0.0-alpha.1. Embedded `VERSION` constant: 4.1.2. | `resources/viewer/htmlwidgets/lib/handlebars-latest.js` | MIT (Expat) | <https://handlebarsjs.com> | Copyright (C) 2011-2017 by Yehuda Katz (file header) |
| Start Bootstrap Resume | 5.0.7 | `resources/viewer/css/resume.css` (Start Bootstrap stylesheet with local customisations) | MIT | <https://startbootstrap.com> | Start Bootstrap, 2013–2019 (file header) |
| Muli (Google Fonts) | Font files `muli_v27` (font version 2.100) | `resources/viewer/vendor/googlefonts/muli_*.ttf`, `Muli-400,400i,800,800i.css` | SIL OFL 1.1 (full text below) | <https://fonts.google.com> | Copyright 2016 The Muli Project Authors (https://github.com/googlefonts/MuliFont) (font metadata) |
| Saira Extra Condensed (Google Fonts) | Font files `sairaec_v11` (font version 0.072) | `resources/viewer/vendor/googlefonts/sairaec_*.ttf`, `Saira_Extra_Condensed-500,700.css` | SIL OFL 1.1 (full text below). Reserved Font Name: "Saira" | <https://fonts.google.com> | Copyright 2016 The Saira Project Authors (omnibus.type@gmail.com), with reserved font name "Saira" (font metadata) |

### Notes on specific components

- **Font Awesome brand icons.** The glyphs in `fa-brands-400` are logos of other companies. They are trademarks of
  their owners and are not covered by the font licence, and must be used only to represent the company, product or
  service they refer to. Font Awesome's own licence statement is at <https://fontawesome.com/license/free>: the SIL OFL
  applies to the web font files and the MIT licence to the CSS, which are the parts shipped here.
- **OHDSI logo.** `resources/viewer/img/ohdsi.png` is the OHDSI logo. It is a trademark of OHDSI and is not released under this
  project's licence. The viewer shows it to identify the upstream dashboard. This project is not affiliated with or
  endorsed by OHDSI.
- **Fonts (OFL).** Muli, Saira Extra Condensed and the Font Awesome brands font are distributed under the SIL Open Font
  License, Version 1.1. The OFL asks that each copy carry the copyright notice and the licence. The notices are in the
  table above and the licence text is [below](#sil-open-font-license-11); this file ships in the release archives and
  the fonts are embedded in the `dqd` binary. The font files are the unmodified Google Fonts files, so the Reserved
  Font Name "Saira" is not in use by a modified version.
- **Handlebars.** No published Handlebars release has the banner version 5.0.0-alpha.1, and the file embeds 28 inline
  source maps (a webpack development build). Its runtime constant says 4.1.2, so it was built between release 4.1.2 and
  the next. The licence is the MIT licence in its header, and the copyright holder is the one the header names.
- **jQuery Easing.** The vendored file is the minified `jquery.easing.min.js` of version 1.4.1. The text of the
  package's `LICENSE` begins "jQuery Easing v1.3.1" because the licence file was not updated for later releases.
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

## Test data

The parity fixtures in `tests/oracle/generated` include three databases that come from the R package Eunomia 2.1.0. See
[docs/test-data.md](docs/test-data.md) for how they were obtained and how to rebuild them.

| Data | Files | Source | Licence |
|---|---|---|---|
| MIMIC-IV demo in the OMOP CDM v0.9 (100 de-identified patients) | `tests/oracle/generated/eunomia/mimic/cdm.sqlite` | [PhysioNet](https://physionet.org/content/mimic-iv-demo-omop/), derived from the [MIMIC-IV Clinical Database Demo](https://physionet.org/content/mimic-iv-demo/), via OHDSI's `EunomiaDatasets` | [Open Data Commons Open Database License v1.0](https://opendatacommons.org/licenses/odbl/) |
| GiBleed (CDM 5.3) and Synthea27Nj (CDM 5.4), synthetic | `tests/oracle/generated/gibleed/sqlite/gibleed-copy.sqlite`, `tests/oracle/generated/duckdb/gibleed/cdm.duckdb`, `tests/oracle/generated/eunomia/synthea27nj/cdm.sqlite` | [Synthea](https://synthetichealth.github.io/synthea/), converted by OHDSI's [ETL-Synthea](https://github.com/OHDSI/ETL-Synthea), via `EunomiaDatasets` | Synthetic data, no real patients. Synthea is Apache-2.0; no restriction on generated data is stated (see below) |

**MIMIC.** The MIMIC database file is licensed under the ODbL v1.0, not under this project's Apache-2.0 licence. PhysioNet's
access policy for the demo, on both of its pages, reads: "Anyone can access the files, as long as they conform to the
terms of the specified license." There is no data use agreement for the demo. The raw demo page records IRB approval and
HIPAA Safe Harbor de-identification. OHDSI's `EunomiaDatasets` repository ships the same dataset with its own
`datasets/MIMIC/LICENSE.txt`, which is the ODbL, and states that its CSV files "were downloaded on Dec 1, 2022 from
https://physionet.org/content/mimic-iv-demo-omop/0.9/".

What the ODbL asks of anyone who redistributes it (sections 4.2 to 4.6 of the licence text):

- Convey the database only under the ODbL, with a copy of the licence or its URI, and keep the licence notices intact.
  This repository meets that with `tests/oracle/generated/eunomia/mimic/LICENSE.txt` (PhysioNet's copy of the licence)
  and `NOTICE.txt` in the same folder.
- A Produced Work, such as a report built from the data, needs a notice that it contains information from the database:
  "Contains information from MIMIC-IV demo data in the OMOP Common Data Model, which is made available here under the
  Open Database License (ODbL)." Creating one needs none; publicly using one does. The results files in that folder
  (`results.json`, `results.csv`, logs) hold aggregate counts and check SQL, not patient rows.
- A Derivative Database that is publicly used must be offered under the ODbL, with the derived database or a file of the
  changes. Incorporating the database in a larger collection (a Collective Database, such as this repository) does not
  require licensing the collection under the ODbL, but the ODbL still applies to the database inside it (sections 4.4
  and 4.5).

**Changes relative to PhysioNet's files.** The committed `cdm.sqlite` is a derivative of PhysioNet's v0.9 CSV files, not
a verbatim copy. `scripts/compare-mimic-with-physionet.py` compares them cell by cell (8,801,873 cells):

- Row counts: the same in every one of the 32 PhysioNet CSV files and the matching tables (no rows added or dropped). The
  seven tables with no PhysioNet file (concept_ancestor, concept_class, concept_synonym, domain, drug_strength,
  relationship, source_to_concept_map) are empty.
- 64-bit ids (person_id, visit_occurrence_id, measurement_id and every other id) are stored as doubles, so most no longer
  match PhysioNet's digits exactly: 1,053,785 cells hold the nearest double and 325,238 a double within two units in the
  last place of it (for example 3589912774911670296 became 3589912774911670272). No key column has collisions: the
  distinct-value counts of 12 id columns are unchanged.
- Dates and datetimes are stored as seconds since 1970 (1,039,570 cells), not as text.
- 358,408 whole numbers are stored as REAL (300 as 300.0); 4,105 numeric-looking text values gain ".0"
  (person_source_value 10009628 became 10009628.0).
- 4,713 text values lose trailing whitespace (for example the concept_code "Labetalol ").
- 12 measurement.operator_concept_id values (the operators > , >= and <, concepts 4172704, 4171755 and 4171756) are NULL
  where PhysioNet's CSV has them. Those three concepts are not in the concept table of the SQLite file.
- 8 dose_era.dose_value cells read 13.992 where the CSV has 13.991999999999999 (binary floating-point noise).
- Everything else is identical: 2,877,714 cells equal and 3,138,320 empty in both, of 8,801,873 compared.

The derivative database is in this repository, which is what section 4.6 asks for, and `NOTICE.txt` in the MIMIC folder
lists the changes. The parity tests ran on this exact file, so they are unaffected; do not use it as a faithful copy of
the MIMIC-IV demo for research.

Cite the data as PhysioNet asks (the full text is in `NOTICE.txt` in the MIMIC folder): Kallfelz, M., Tsvetkova, A.,
Pollard, T., Kwong, M., Lipori, G., Huser, V., Osborn, J., Hao, S., & Williams, A. (2021). *MIMIC-IV demo data in the OMOP
Common Data Model* (version 0.9). PhysioNet. <https://doi.org/10.13026/p1f5-7x35>, together with the standard PhysioNet
citation. This project does not include, and must not include, the full MIMIC-IV, which needs PhysioNet credentialed
access.

**Synthea-based datasets.** Synthea is licensed under Apache-2.0 (copyright The MITRE Corporation). Its README and FAQ
state that the data is synthetic ("No individual-level data was used in creating it") and impose no restriction on the
generated records, but they do not say whether the Apache licence extends to generated output. The FAQ's preferred
citation is Walonoski, J., et al. (2018). Synthea: An approach, method, and software mechanism for generating synthetic
patients and the synthetic electronic health care record. *JAMIA* 25(3), 230-238.
<https://doi.org/10.1093/jamia/ocx079>. OHDSI's `EunomiaDatasets` holds no licence or readme file in the `GiBleed` or
`Synthea27Nj` folders; its root `LICENSE` is Apache-2.0 and does not name the datasets.

## jQuery Easing licence (BSD-3-Clause)

The text below is the `LICENSE` file of `jquery.easing@1.4.1`, unchanged. It holds both notices: the plugin and the easing
equations.

```text
jQuery Easing v1.3.1 - http://gsgd.co.uk/sandbox/jquery/easing/

Uses the built in easing capabilities added In jQuery 1.1
to offer multiple easing options

TERMS OF USE - jQuery Easing

Open source under the BSD License.

Copyright © 2008 George McGinley Smith
All rights reserved.

Redistribution and use in source and binary forms, with or without modification,
are permitted provided that the following conditions are met:

Redistributions of source code must retain the above copyright notice, this list of
conditions and the following disclaimer.
Redistributions in binary form must reproduce the above copyright notice, this list
of conditions and the following disclaimer in the documentation and/or other materials
provided with the distribution.

Neither the name of the author nor the names of contributors may be used to endorse
or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY
EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL,
EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE
GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED
AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED
OF THE POSSIBILITY OF SUCH DAMAGE.



TERMS OF USE - EASING EQUATIONS
Open source under the BSD License.
Copyright © 2001 Robert Penner
All rights reserved.
Redistribution and use in source and binary forms, with or without modification
are permitted provided that the following conditions are met:
Redistributions of source code must retain the above copyright notice, this list of
conditions and the following disclaimer.
Redistributions in binary form must reproduce the above copyright notice, this list
of conditions and the following disclaimer in the documentation and/or other materials
provided with the distribution.
Neither the name of the author nor the names of contributors may be used to endorse
or promote products derived from this software without specific prior written permission.
THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY
EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL,
EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE
GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED
AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED
OF THE POSSIBILITY OF SUCH DAMAGE.
```

## SIL Open Font License 1.1

This licence applies to the Muli, Saira Extra Condensed and Font Awesome (brands) font files listed above. The copyright
notices that go with it are:

- Muli: Copyright 2016 The Muli Project Authors (https://github.com/googlefonts/MuliFont)
- Saira Extra Condensed: Copyright 2016 The Saira Project Authors (omnibus.type@gmail.com), with reserved font name "Saira"
- Font Awesome Free brands font: Copyright (c) Font Awesome

The licence text is the standard SIL OFL 1.1, also at <https://openfontlicense.org>:

```text
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
```

## Open items

1. **Where the MIMIC changes were made.** The committed file differs from PhysioNet's CSVs as listed above. This repository does
   not show whether Eunomia's loader made them or they came later. Compare the file with the `MIMIC_5.3.zip` in OHDSI's
   `EunomiaDatasets` (or with the output of `Eunomia::getDatabaseFile("MIMIC", cdmVersion = "5.3")`) to find out. The ODbL
   position does not depend on it: either way the file is a derivative, offered here under the ODbL.
