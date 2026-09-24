# Security policy

Ostrin is an experimental 0.x language and compiler. Security fixes are
welcome, but the project does not yet promise a stable security boundary for
every compiler feature or generated native program.

## Supported versions

| Version | Supported |
| --- | --- |
| `main` | Yes |
| Latest `0.1.x` release | Best effort |
| Older releases | No |

The browser playground runs the interpreter in WebAssembly. Native compilation
and execution are separate desktop operations and should only be used with
source code and dependencies you trust.

## Reporting a vulnerability

Please report suspected vulnerabilities privately through
[GitHub Security Advisories](https://github.com/sircalch/Ostrin/security/advisories/new).
If that channel is unavailable, open a minimal issue asking for a private
contact method without including exploit details.

Include the affected commit or release, operating system, compiler target, a
small reproducer, expected and observed behavior, and any relevant command
output. Please do not publish a working exploit before a fix is available.

We will acknowledge a report when we can, reproduce it on the supported
toolchain, and credit the reporter in the release notes unless they request
anonymity. There is currently no bug bounty program.

## Dependency and CI checks

Every pull request runs the Rust dependency advisory scan and CodeQL workflow.
The native sanitizer workflow is Linux-only because generated C programs rely
on a GNU-compatible compiler and platform runtime. Fuzzing of lexer, parser,
checker, HIR, IR, and native-lowering entry points is part of the compiler differential test suite;
the `OSTRIN_FUZZ_ROUNDS` environment variable controls deeper local runs.
