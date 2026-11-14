# Security policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately. Do not open a public issue or pull request for them.

Use GitHub's private vulnerability reporting for this repository:
<https://github.com/ABorakati/data-quality-dashboard-rs/security/advisories/new>
(Security tab, then "Report a vulnerability"). Include the version or commit, the steps to reproduce, and the
impact as you understand it. Remove patient data, credentials and connection strings from anything you send.

The maintainer will reply as soon as practical and will keep you informed while the issue is worked on. Once a
fix is released, the advisory is published with credit to the reporter, unless you ask not to be named.

Problems in upstream OHDSI DataQualityDashboard, DatabaseConnector, SqlRender or CommonDataModel belong to
those projects. Report them to OHDSI. This port is not affiliated with OHDSI.

## Supported versions

| Version | Security fixes |
|---|---|
| `main` branch | Yes |
| Latest tagged release (`v*`) | Yes |
| Older tagged releases | No. Upgrade to the latest release. |

## Security model

`dqd` is a command-line tool and a library. It is meant to run on a machine where the user already has access to
the OMOP CDM, and it has the same access as the credentials it is given.

- **It runs SQL against your database.** `dqd` generates SQL from the upstream check templates and executes it on
  the database named in the options file, using the credentials supplied. Point it only at databases you are
  authorised to query. Prefer a read-only account on the CDM schema, with write access only to the results schema.
- **The CDM can contain patient data, and so can outputs.** Results JSON, CSV and result tables hold aggregate
  counts, but check error messages, log files, SQL-only exports and error reports can echo values from the
  database. Keep these outputs on the machine and out of version control, tickets and shared folders unless you
  have checked them.
- **Credentials.** The `--password` flag puts the password in the command line, which shell history and process
  listings can show. Prefer an options file that is not in version control and is readable only by its owner.
  Do not commit options files that contain passwords.
- **The viewer has no authentication.** `dqd viewer` serves result files and read-only APIs over HTTP. It binds
  to `127.0.0.1` by default. Use it on a trusted machine only. Do not bind it to a network interface, a shared
  host or a public address unless a separate access-control layer, such as an authenticating reverse proxy, sits in
  front of it. Its results can contain the same aggregate data described above.
- **Downloaded binaries.** Release archives come with a `SHA256SUMS` file. Check it before running a binary.

## Scope

Within scope: memory safety or crashes on crafted input, SQL injection or unintended SQL execution from option
values or CDM data, path traversal or unintended file writes, the viewer serving files outside the results
directory, and credential handling that leaks secrets.

Out of scope: misconfiguration of a database server, and problems that arise only because someone exposed the
viewer to an untrusted network against the guidance above.
