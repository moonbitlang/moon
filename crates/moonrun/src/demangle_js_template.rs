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

pub(crate) const DEMANGLE_JS_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/template/demangle.js"
));

#[cfg(test)]
mod tests {
    use super::DEMANGLE_JS_TEMPLATE;
    use std::sync::Once;

    const DEMANGLE_FN_NAME: &str = "__moonbit_demangle_mangled_function_name";

    fn init_v8_once() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            v8::V8::set_flags_from_string("--experimental-wasm-exnref");
            v8::V8::set_flags_from_string("--experimental-wasm-imported-strings");
            let platform = v8::new_default_platform(0, false).make_shared();
            v8::V8::initialize_platform(platform);
            v8::V8::initialize();
        });
    }

    fn run_demangle_in_v8(input: &str) -> String {
        init_v8_once();
        let isolate = &mut v8::Isolate::new(Default::default());
        let scope = &mut v8::HandleScope::new(isolate);
        let context = v8::Context::new(scope, Default::default());
        let scope = &mut v8::ContextScope::new(scope, context);

        let script = v8::String::new(scope, DEMANGLE_JS_TEMPLATE).unwrap();
        let script = v8::Script::compile(scope, script, None).unwrap();
        script.run(scope).unwrap();

        let global = scope.get_current_context().global(scope);
        let func_name = v8::String::new(scope, DEMANGLE_FN_NAME).unwrap();
        let func = global.get(scope, func_name.into()).unwrap();
        let func: v8::Local<v8::Function> = func.try_into().unwrap();

        let arg = v8::String::new(scope, input).unwrap();
        let result = func.call(scope, global.into(), &[arg.into()]).unwrap();
        result.to_string(scope).unwrap().to_rust_string_lossy(scope)
    }

    #[test]
    fn js_demangler_template_exports_function() {
        assert_eq!(run_demangle_in_v8("_M0FP13pkg3foo"), "@pkg.foo".to_string());
    }

    #[test]
    fn js_demangler_template_matches_rust_demangler() {
        let samples = [
            "_M0FP13pkg3foo",
            "_M0MP13pkg4Type3bar",
            "_M0IP13pkg4ImplP13pkg5Trait3run",
            "_M0EP13pkg4TypeP14util3new",
            "_M0TP13pkg4Type",
            "_M0L6_2atmpS9127",
            "_M0FP15myapp8try__mapGiEHRP15myapp7MyError",
            "_M0FP15myapp3runGVWiEsE",
            "_M0FP15myapp8try__runGWiEsQRP15myapp7MyErrorE",
            "_M0EP311moonbitlang4core7builtin3IntP15myapp6double",
            "_M0FPB30output_2eflush__segment_7c4024",
            "_M0L61_24username_2fhello_2fmain_2eabort__via__closure_2einner_2efnS271",
            "_M0FP13pkg3foo$closure.data",
            "_M0FP13pkg3foo@123",
            "_M0FP13pkg3foo.",
            "$_M0FP13pkg3foo",
            "_M0FP15myapp7try_mapGiE",
            "_M0FP314d_2dh24moonbit_2dscatter_2dplot14scatter_2dplot28gen__scatter__plot__graphics",
            "plain",
        ];

        for sample in samples {
            let expected = moonutil::demangle::demangle_mangled_function_name(sample);
            let actual = run_demangle_in_v8(sample);
            assert_eq!(actual, expected, "sample: {sample}");
        }
    }
}
