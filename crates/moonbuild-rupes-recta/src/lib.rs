// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

/*!
    Rupes Recta is the new build graph generator for MoonBuild.

    ## Quickstart

    You can find high-level abstractions in modules [`resolve`] and [`compile`],
    which splits the compilation process in two parts:

    - [`resolve`] Builds an in-memory representation of all modules and packages
        that needs to be used during the compile process, as well as the
        dependency relationship between them. This part is always performed
        without affected by the user input.

    - [`compile`] takes in the resolved environment, and builds a [`n2`] build
        graph for execution. This part converts the intent of the user into the
        actual build commands.

    Of all intents of the user, one is so different from the rest: `moon fmt`.
    The formatter only needs the list of files to run, regardless of whether the
    project is compilable or not. Thus, it's worth its own module at [`fmt`]
    which bypasses the rest of the pipeline below.

    ## Design

    The overall design of Rupes Recta is very similar to that of Rust's build
    system, [Cargo](https://docs.rs/cargo/latest/cargo/), although it is
    designed independently without referencing much from it.

    The modules within this project aim to be implemented under the
    [*Single-responsibility
    Principle*](https://en.wikipedia.org/wiki/Single-responsibility_principle).
    Each module should perform one and only one step within the build program,
    and different steps of building should be done separately instead of being
    coupled together within one step.

    The rough steps (with Rupes Recta) of building a MoonBit program are:

    1. Read the `mooncakes.io` registry and resolve the *module* dependency
        graph ([`mooncake::resolver`]).
    2. Download the required dependency to local cache folders
        ([`mooncake::pkg::install`]).
    3. Discover packages within modules ([`crate::discover`]). This is different
        from many package managers -- the package distribution unit ("module")
        is different from the compile unit ("package").
    4. Resolve the *package* dependency graph ([`crate::pkg_solve`]).
    5. Get the list of top-level build actions from user input.
    6. From this list of build actions, resolve the whole abstract build graph
        that represents the list of actions to be executed
        ([`crate::build_plan`]).
    7. Lower the build graph to a concrete one acceptable by [`n2`] (which is an
        in-process `ninja` equivalent) ([`crate::build_lower`]).
    8. Execute the build graph using `n2`.

    Additional information about the build process, project layout, special
    cases, and random quirks of build systems can be found in the repository's
    documentation, at `/docs/dev/reference`.

    ## Alternative design

    An alternative is proposed, but not implemented, with replacing `n2` with
    a hand-rolled, in-process executor that directly works on the abstract build
    graph instead of requiring to lower every command beforehand.

    ## Logging

    This crate uses the `log` crate for structured logging. Enable logging to see
    detailed information about the build process:

    - `info` level: High-level progress and completion messages
    - `debug` level: Detailed operation information, counts, and intermediate results
    - `trace` level: Very detailed information about individual operations

    Initialize a logger (such as `env_logger`) to see these messages.

    ## Maintainers

    Except for code the [`discover`] module, **no** file I/O operation should be
    done in this crate.
*/

#![warn(clippy::unwrap_used)] // We prefer clear panic messages

pub mod build_lower;
pub mod build_plan;
pub mod discover;
pub mod model;
pub mod pkg_name;
pub mod pkg_solve;

// High-level actions
pub mod compile;
pub mod resolve;

// Formatter
pub mod fmt;

// Random utilities
pub mod cond_comp;
pub mod intent;
pub mod metadata;
pub mod prebuild;
mod special_cases;
pub mod util;

// Reexports
pub use compile::{compile, CompileConfig, CompileOutput};
pub use resolve::{resolve, ResolveConfig, ResolveOutput};
