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

use super::MoonBuildCli;
use clap::CommandFactory;
use clap_complete::{generate, Shell};
use moonutil::cli::UniversalFlags;
use std::io;

/// Generate shell completion for bash/elvish/fish/pwsh/zsh to stdout
#[derive(Debug, clap::Parser)]
#[clap(after_help = r"
Discussion:
    Enable tab completion for Bash, Fish, Zsh, or PowerShell
    The script is output on `stdout`, allowing one to re-direct the
    output to the file of their choosing. Where you place the file
    will depend on which shell, and which operating system you are
    using. Your particular configuration may also determine where
    these scripts need to be placed.

    Here are some common set ups for the three supported shells under
    Unix and similar operating systems (such as GNU/Linux).

    Bash:

    Completion files are commonly stored in `/etc/bash_completion.d/` for
    system-wide commands, but can be stored in
    `~/.local/share/bash-completion/completions` for user-specific commands.
    Run the command:

        $ mkdir -p ~/.local/share/bash-completion/completions
        $ moon shell-completion --shell bash >> ~/.local/share/bash-completion/completions/moon

    This installs the completion script. You may have to log out and
    log back in to your shell session for the changes to take effect.

    Bash (macOS/Homebrew):

    Homebrew stores bash completion files within the Homebrew directory.
    With the `bash-completion` brew formula installed, run the command:

        $ mkdir -p $(brew --prefix)/etc/bash_completion.d
        $ moon shell-completion --shell bash > $(brew --prefix)/etc/bash_completion.d/moon.bash-completion

    Fish:

    Fish completion files are commonly stored in
    `$HOME/.config/fish/completions`. Run the command:

        $ mkdir -p ~/.config/fish/completions
        $ moon shell-completion --shell fish > ~/.config/fish/completions/moon.fish

    This installs the completion script. You may have to log out and
    log back in to your shell session for the changes to take effect.

    Zsh:

    ZSH completions are commonly stored in any directory listed in
    your `$fpath` variable. To use these completions, you must either
    add the generated script to one of those directories, or add your
    own to this list.

    Adding a custom directory is often the safest bet if you are
    unsure of which directory to use. First create the directory; for
    this example we'll create a hidden directory inside our `$HOME`
    directory:

        $ mkdir ~/.zfunc

    Then add the following lines to your `.zshrc` just before
    `compinit`:

        fpath+=~/.zfunc

    Now you can install the completions script using the following
    command:

        $ moon shell-completion --shell zsh > ~/.zfunc/_moon

    You must then either log out and log back in, or simply run

        $ exec zsh

    for the new completions to take effect.

    Custom locations:

    Alternatively, you could save these files to the place of your
    choosing, such as a custom directory inside your $HOME. Doing so
    will require you to add the proper directives, such as `source`ing
    inside your login script. Consult your shells documentation for
    how to add such directives.

    PowerShell:

    The powershell completion scripts require PowerShell v5.0+ (which
    comes with Windows 10, but can be downloaded separately for windows 7
    or 8.1).

    First, check if a profile has already been set

        PS C:\> Test-Path $profile

    If the above command returns `False` run the following

        PS C:\> New-Item -path $profile -type file -force

    Now open the file provided by `$profile` (if you used the
    `New-Item` command it will be
    `${env:USERPROFILE}\Documents\WindowsPowerShell\Microsoft.PowerShell_profile.ps1`

    Next, we either save the completions file into our profile, or
    into a separate file and source it inside our profile. To save the
    completions into our profile simply use

        PS C:\> moon shell-completion --shell powershell >>
        ${env:USERPROFILE}\Documents\WindowsPowerShell\Microsoft.PowerShell_profile.ps1

    This discussion is taken from `rustup completions` command.
")]
pub struct ShellCompSubCommand {
    /// The shell to generate completion for
    #[clap(value_enum, long, ignore_case = true, value_parser = clap::builder::EnumValueParser::<Shell>::new(), default_value_t = Shell::from_env().unwrap_or(Shell::Bash), value_name = "SHELL")]
    pub shell: Shell,
}

pub fn gen_shellcomp(_cli: &UniversalFlags, cmd: ShellCompSubCommand) -> anyhow::Result<i32> {
    if _cli.dry_run {
        anyhow::bail!("this command has no side effects, dry run is not needed.")
    }
    let mut _moon = MoonBuildCli::command();
    generate(cmd.shell, &mut _moon, "moon", &mut io::stdout());
    Ok(0)
}
