;; Regenerate with:
;; wasm-tools parse v8_external_finalizers.wat -o v8_external_finalizers.wasm
;;
;; The loop forces V8 to collect host-created externrefs before isolate
;; teardown, exercising both regular finalization and teardown finalization.
(module
  (import "__moonbit_time_unstable" "instant_now"
    (func $instant_now (result externref)))
  (import "__moonbit_rand_unstable" "stdrng_seed_from_u64"
    (func $stdrng_seed_from_u64 (param i32) (result externref)))
  (func (export "_start")
    (local $i i32)
    (loop $allocate
      call $instant_now
      drop
      local.get $i
      call $stdrng_seed_from_u64
      drop
      local.get $i
      i32.const 1
      i32.add
      local.tee $i
      i32.const 100000
      i32.lt_u
      br_if $allocate)))
