#!/bin/sh
# Exercise Moon's record/export protocol without starting Instruments. Recording
# still executes the real workload so argument, cwd, and stdout handling matter.
set -eu

[ "$1" = xctrace ]
action=$2
shift 2

output=
input=
stdout=
template=
xpath=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --quiet | --no-prompt) shift ;;
        --template) template=$2; shift 2 ;;
        --output) output=$2; shift 2 ;;
        --input) input=$2; shift 2 ;;
        --target-stdout) stdout=$2; shift 2 ;;
        --xpath) xpath=$2; shift 2 ;;
        --launch)
            [ "$action" = record ]
            [ "$2" = -- ]
            shift 2
            break ;;
        *)
            printf 'unexpected xctrace argument: %s\n' "$1" >&2
            exit 1 ;;
    esac
done

[ -n "$output" ]
case "$action" in
    record)
        [ "$template" = 'Time Profiler' ]
        [ -n "$stdout" ]
        [ "$#" -gt 0 ]
        mkdir "$output"
        "$@" > "$stdout"
        # Export must consume the trace from this successful recording.
        printf 'recorded\n' > "$output/fake-recording"
        ;;
    export)
        [ "$#" -eq 0 ]
        [ "$xpath" = '/trace-toc/run[@number="1"]/data/table[@schema="time-profile"]' ]
        [ -f "$input/fake-recording" ]
        cat > "$output" <<'XML'
<trace-query-result><node>
<row>
  <thread-state id="1" fmt="Running">Running</thread-state>
  <weight id="2" fmt="1.00 ms">1000000</weight>
  <stack id="3"><frame id="4" name="_M0fake_workload" addr="0x1"/></stack>
</row>
<row><thread-state ref="1"/><weight ref="2"/><stack ref="3"/></row>
</node></trace-query-result>
XML
        ;;
    *)
        printf 'unexpected xctrace action: %s\n' "$action" >&2
        exit 1 ;;
esac
