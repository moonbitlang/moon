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

pub struct CommandBuilder {
    command: String,
    args: Vec<String>,
}

impl CommandBuilder {
    pub fn new(command: &str) -> CommandBuilder {
        CommandBuilder {
            command: command.into(),
            args: Vec::new(),
        }
    }

    pub fn arg_with_cond(&mut self, cond: bool, arg: &str) -> &mut CommandBuilder {
        if cond {
            self.args.push(arg.into());
        }
        self
    }

    pub fn lazy_args_with_cond(
        &mut self,
        cond: bool,
        args: impl FnOnce() -> Vec<String>,
    ) -> &mut CommandBuilder {
        if cond {
            for arg in args().into_iter() {
                self.args.push(arg);
            }
        }
        self
    }

    pub fn args_with_cond<I, S>(&mut self, cond: bool, args: I) -> &mut CommandBuilder
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if cond {
            for arg in args {
                self.args.push(arg.into());
            }
        }
        self
    }

    pub fn arg(&mut self, arg: &str) -> &mut CommandBuilder {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut CommandBuilder
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for arg in args {
            self.args.push(arg.into());
        }
        self
    }

    pub fn args_with_prefix_separator<I, S>(
        &mut self,
        args: I,
        separator: &str,
    ) -> &mut CommandBuilder
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for arg in args {
            self.args.push(separator.into());
            self.args.push(arg.into());
        }
        self
    }

    pub fn build(&self) -> String {
        let mut cmd = self.command.clone();
        for arg in self.args.iter() {
            cmd.push(' ');
            if arg.contains(' ') {
                cmd.push('"');
                cmd.push_str(arg);
                cmd.push('"');
            } else {
                cmd.push_str(arg);
            }
        }
        cmd
    }
}
