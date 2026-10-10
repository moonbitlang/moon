// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Small construction helpers shared by V8 adapters.

use v8::{FunctionCallback, FunctionCallbackArguments, HandleScope, Local, Object, Value};

pub(crate) trait ScopeExt<'s> {
    fn string(&mut self, value: &str) -> Local<'s, v8::String>;
}

impl<'s> ScopeExt<'s> for HandleScope<'s> {
    fn string(&mut self, value: &str) -> Local<'s, v8::String> {
        v8::String::new(self, value).unwrap()
    }
}

pub(crate) trait ArgsExt {
    fn string_lossy(&self, scope: &mut HandleScope, index: i32) -> String;
}

impl<'s> ArgsExt for FunctionCallbackArguments<'s> {
    fn string_lossy(&self, scope: &mut HandleScope, index: i32) -> String {
        self.get(index)
            .to_string(scope)
            .unwrap()
            .to_rust_string_lossy(scope)
    }
}

pub(crate) trait ObjectExt<'s> {
    fn set_value(&self, scope: &mut HandleScope<'s>, name: &str, value: Local<'s, Value>);
    fn set_func(
        &self,
        scope: &mut HandleScope<'s>,
        name: &str,
        callback: impl v8::MapFnTo<FunctionCallback>,
    );
    fn child(&self, scope: &mut HandleScope<'s>, name: &str) -> Local<'s, Object>;
}

impl<'s> ObjectExt<'s> for Local<'s, Object> {
    fn set_value(&self, scope: &mut HandleScope<'s>, name: &str, value: Local<'s, Value>) {
        let key = scope.string(name);
        self.set(scope, key.into(), value);
    }

    fn set_func(
        &self,
        scope: &mut HandleScope<'s>,
        name: &str,
        callback: impl v8::MapFnTo<FunctionCallback>,
    ) {
        let func = v8::FunctionTemplate::new(scope, callback)
            .get_function(scope)
            .unwrap();
        self.set_value(scope, name, func.into());
    }

    fn child(&self, scope: &mut HandleScope<'s>, name: &str) -> Local<'s, Object> {
        let child = v8::Object::new(scope);
        self.set_value(scope, name, child.into());
        child
    }
}
