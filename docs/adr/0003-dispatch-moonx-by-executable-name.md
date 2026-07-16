# Dispatch Moonx By Executable Name

MoonBuild will compile one `moon` binary and select the `moonx` command-line interface from the invoked executable name (`argv[0]`). Distributors will provide `moonx` as a hard link or identical copy rather than a second Cargo binary, keeping one executable implementation and one release artifact while still providing a first-class `moonx` entrance. The external installer change is follow-up work; this repository owns dispatch behavior and its tests.
