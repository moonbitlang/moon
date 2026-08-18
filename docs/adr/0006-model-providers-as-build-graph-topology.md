---
status: accepted
---

# Model Providers as Build Graph Topology

MoonBuild models a semantic Build Plan as actions that consume and produce
Build Artifacts. An artifact's identity describes the logical build result;
the action that provides it and the physical location that realizes it do not
belong to that identity. Consumers name artifacts, and provider edges express
which planned actions produce them.

This decision defines the target model. Implemented-behavior references remain
authoritative while the current split representations are migrated and removed.

Concrete package files are not Build Artifacts. Discovery supplies file sets,
and package prebuild declarations contribute output paths to those sets without
classifying a path as authored or generated. Build Target Projection keeps the
selected sets grouped by their compiler behavior until lowering expands them
into concrete Execution Action inputs. Whether a path currently exists does
not change the plan.

Execution Plan construction centrally matches concrete action inputs with
Declared Action Outputs of the same path. This creates execution dependencies
from compiler actions to package prebuild actions without an additional
package-file identity, provider registry, or caller-side produced-versus-
external classification. Inputs with no matching declared output remain
observations supplied across the Execution Plan boundary.

Build Artifacts are single-assignment within one Build Plan. Every required
artifact has exactly one provider. Distinct artifacts remain distinct even if
their current physical forms use the same extension or layout convention.

## Consequences

- The artifact registry owns Build Artifact membership and the
  artifact-to-provider index. An absent required artifact is a planning error.
- Action metadata retains input roles and any semantically significant order
  independently of provider topology. Command behavior never branches on a
  provider's origin or presence.
- Lowering realizes Build Artifact identities as concrete paths but does not
  reconstruct provenance from paths, Resolve Output, or command arguments.
  Package relationships and other lowering context may determine how a
  realized artifact path is rendered, such as an import alias or virtual
  package flag, but may not reconstruct that path or add a second physical
  input for the same dependency.
- Execution actions expose one input-path relation with observation kinds such
  as a regular file or recursive interface bundle. Execution-plan construction
  centrally resolves matching Declared Action Outputs; callers do not classify
  inputs as produced or external in advance.
- Within one Execution Plan, one concrete output path has at most one producer.
  Matching an input path to that output creates an execution dependency;
  matching paths never merges distinct semantic Build Artifacts.
- Installed standard-library interfaces are known toolchain inputs, selected
  by the standard-library domain rule rather than inferred from a missing
  artifact provider. Recursive `-std-path` observation remains an explicit
  execution observation with its own command role and cache semantics.

## Considered Options

- Encoding package prebuild outputs as Build Artifacts, such as
  `PrebuildOutput`, was rejected. Package file sets are concrete compiler
  inputs, and set membership already removes duplication between discovered
  paths and declared prebuild outputs.
- Expanding `Build Artifact` to mean every tracked file and execution
  observation was rejected. Semantic build results, concrete package inputs,
  recursive directory observations, and physical-only outputs have different
  request and execution policies.
- Splitting action inputs into produced dependencies and external observations
  at consumer call sites was rejected because it makes consumers know graph
  topology. Execution Plan construction can resolve that distinction centrally
  from concrete input and output paths.
- Adding source-specific provider modes such as `StandardLibrary` was rejected.
  The installed standard library is already explicit invocation context; it
  does not need a second artifact-provider taxonomy.
