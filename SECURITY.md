# Security

## Reporting a vulnerability

Please **do not** open a **public** GitHub issue for undisclosed security vulnerabilities in **this repository** (Rust workspace, CI, or documented integration with submodules).

**Preferred:** use [GitHub private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability) for **leek-wars/leekscript-env** if the feature is enabled for the repository.

**Alternative:** contact the maintainers through a **private** channel they publish for this project (e.g. org security contact). If no channel is listed, open a **draft** or **minimal** issue asking for a secure contact (without exploit details).

For vulnerabilities that exist **only** in upstream **`leek-wars/`** or **`leek-wars-generator/`** content, report them to **those** projects per their security policies.

## Scope (high level)

- Rust crates under **`crates/`**, root **`Cargo.toml`** / **`Cargo.lock`**, **`deny.toml`**, and **`.github/workflows/`**.
- **Secrets:** keep tokens (e.g. **`LEEKWARS_TOKEN`**) in environment or a secret manager; do not commit them or paste them into issues. See [environment variables](docs/guides/environment.md).

## Supply chain

Workspace **`deny.toml`** and CI run **`cargo-deny`** for licenses and advisories; see [CI & quality gates](docs/operations/ci-and-quality-gates.md).
