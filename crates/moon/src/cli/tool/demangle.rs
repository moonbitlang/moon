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

/// Demangle MoonBit symbol names.
#[derive(Debug, clap::Parser)]
pub(crate) struct DemangleSubcommand {
    /// Mangled names to demangle.
    #[clap(value_name = "NAME", required = true)]
    names: Vec<String>,
}

pub(crate) fn run_demangle(cmd: DemangleSubcommand) -> anyhow::Result<i32> {
    for name in cmd.names {
        println!(
            "{}",
            moonutil::demangle::demangle_mangled_function_name(&name)
        );
    }
    Ok(0)
}
