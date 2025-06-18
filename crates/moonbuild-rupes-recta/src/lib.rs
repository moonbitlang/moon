/*!
    Rupes Recta is the new build graph generator for MoonBuild.

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
*/

#![warn(clippy::unwrap_used)] // We prefer clear panic messages

pub mod build_lower;
pub mod build_plan;
mod cond_comp;
pub mod discover;
pub mod model;
pub mod pkg_name;
pub mod pkg_solve;
