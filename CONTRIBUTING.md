# Contributing to IronLAB

Contributions are welcome. This file covers the one thing that must be agreed before a contribution can be accepted, which is how it is licensed, and points at the conventions that govern everything else.

## Working on the code

IronLAB is a Cargo workspace. `cargo test --workspace` runs the whole suite, and `cargo run -p ironlab-gallery -- export <directory>` renders the example figures. Some tests need things the machine may not have: [buf](https://buf.build) to check the generated Protocol Buffers, `poppler` and `ghostscript` to validate exported PDFs, and a graphics adapter to render offscreen. Each of those tests reports that it is skipping rather than failing when what it needs is absent, unless the matching variable — `IRONLAB_REQUIRE_BUF`, `IRONLAB_REQUIRE_PDF_TOOLS` or `IRONLAB_REQUIRE_GPU` — is set, as all three are in continuous integration. Set them locally if you want the same strictness.

Install the hooks with `pre-commit install` before your first commit. They run `cargo fmt` and `cargo clippy` over the workspace, and check that the commit message and branch name follow the conventions.

How branches are named and based, how commit messages are written, how the version is calculated, and how pull requests are opened and titled are all described in the conventions section of the documentation: [branching](https://ironlab.org/conventions/git-branching/), [commits and versioning](https://ironlab.org/conventions/git-commits-and-versioning/) and [pull requests](https://ironlab.org/conventions/git-pull-requests/).

## Licensing of contributions

IronLAB is released under the GNU Affero General Public License, either version 3 or (at your option) any later version. The third-party files in `crates/ironlab-gallery/assets` are the one exception: each keeps the licence of its author, as [the notice in that directory](crates/ironlab-gallery/assets/LICENSE.md) records.

By opening a pull request against this repository, you confirm that you wrote the contribution yourself or otherwise hold the right to submit it, and you agree to both of the following.

1. Your contribution is licensed to the project, and to everyone who receives the project, under the AGPL, version 3 or later: the same terms as the rest of IronLAB.
2. You additionally and irrevocably grant the maintainers of IronLAB the right to release your contribution under any other licence they later adopt for IronLAB or for any part of it, including a licence more permissive than the AGPL.

### Why the second grant is asked for

**There is at present no intention to change the licence of any part of IronLAB.** The second grant exists so that the decision remains available in future, not because it has been taken or is expected.

The reason it has to be asked for now, rather than at the point it is needed, is that relicensing requires the agreement of everyone who holds copyright in the code. If contributions are accepted under the AGPL alone, then every past contributor acquires an effective veto over any future licensing decision, and exercising that decision would mean tracing each of them and obtaining their individual consent. In practice that is rarely achievable, and a project that has not asked for this grant from the beginning is usually fixed on its original licence permanently.

The specific facility being preserved is the ability to make parts of IronLAB more permissive. The AGPL is a deliberate choice for an application, but it is a demanding licence for a library that other people embed in their own work, and the lower layers of IronLAB — the figure model, the typesetting, the scene compiler and the PDF exporter — are libraries of exactly that kind. Should the project later conclude that some of those layers serve their purpose better under permissive terms, this grant is what makes that possible without abandoning the contributions made up to that point.

The grant concerns future releases only. Every version already published under the AGPL stays available under the AGPL, for everyone who has it and everyone who obtains it afterwards. A licence granted with a release cannot be withdrawn by a later one.

If you are contributing on behalf of an employer, please make sure they are content with both grants before you open the pull request.
