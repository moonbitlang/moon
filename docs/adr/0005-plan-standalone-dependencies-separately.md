---
status: accepted
---

# Use Unified Planning and Execution for Standalone Builds

Standalone scripts and ordinary projects use the same planning, lowering, and
execution path. Each backend produces one complete `BuildPlan` and
`ExecutionPlan`; multi-backend invocations compose those plans before one n2
execution. Synthesizing a project for a script does not introduce an execution
mode or a separate dependency graph.

This revises the earlier decision to execute all dependency work before script
work. That adapter duplicated orchestration without enabling cross-invocation
artifact reuse. Both phases were already planned upfront and used the same n2
database. Unified execution preserves dependency ordering through producer
edges and allows otherwise independent actions to proceed without a phase
barrier. Failure handling and diagnostic limits follow the ordinary executor.

## Dependency Semantics

Lowering retains a semantic `is_dependency_artifact` marker on artifact
realizations. Artifacts in the resolved project's root modules belong to the
project; artifacts in other resolved modules belong to its dependencies,
including local-path dependencies. Outputs without module ownership remain
unmarked. This role does not change artifact identity, provider relationships,
execution order, freshness, or cache eligibility.

Faster script builds through validated dependency-artifact reuse remain an
intended experiment. The marker preserves the domain distinction that such
an experiment may interpret. No phase partitioner, execution-mode interface,
or artifact-cache execution is introduced in anticipation of that experiment.

## Consequences

- Standalone builds use the same complete build rules and executor as projects.
- Dependency source acquisition and artifact paths remain unchanged.
- Persistent scripts retain their target-directory n2 database and incremental
  reuse. Inline and stdin scripts still use temporary projects; their build
  outputs are not retained across invocations.
- Dependency artifacts remain distinguishable without selecting an execution
  strategy. A future reuse implementation can consume that metadata and the
  existing artifact/provider relationships.
- New execution structure requires evidence from the reuse experiment, rather
  than the presence of a standalone input.
