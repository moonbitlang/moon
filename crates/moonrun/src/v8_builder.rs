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

use v8::{FunctionCallback, FunctionCallbackArguments, Local, Object, Value};

pub(crate) trait ScopeExt<'s> {
    fn string(&mut self, value: &str) -> Local<'s, v8::String>;
}

impl<'s, 'i> ScopeExt<'s> for v8::PinScope<'s, 'i> {
    fn string(&mut self, value: &str) -> Local<'s, v8::String> {
        v8::String::new(self, value).unwrap()
    }
}

pub(crate) trait ArgsExt {
    fn string_lossy(&self, scope: &mut v8::PinScope<'_, '_>, index: i32) -> String;
}

impl<'s> ArgsExt for FunctionCallbackArguments<'s> {
    fn string_lossy(&self, scope: &mut v8::PinScope<'_, '_>, index: i32) -> String {
        self.get(index)
            .to_string(scope)
            .unwrap()
            .to_rust_string_lossy(scope)
    }
}

pub(crate) trait ObjectExt<'s> {
    fn set_value(&self, scope: &mut v8::PinScope<'s, '_>, name: &str, value: Local<'s, Value>);
    fn set_func(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        name: &str,
        callback: impl v8::MapFnTo<FunctionCallback>,
    );
    fn child(&self, scope: &mut v8::PinScope<'s, '_>, name: &str) -> Local<'s, Object>;
}

impl<'s> ObjectExt<'s> for Local<'s, Object> {
    fn set_value(&self, scope: &mut v8::PinScope<'s, '_>, name: &str, value: Local<'s, Value>) {
        let key = scope.string(name);
        self.set(scope, key.into(), value);
    }

    fn set_func(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        name: &str,
        callback: impl v8::MapFnTo<FunctionCallback>,
    ) {
        let func = v8::FunctionTemplate::new(scope, callback)
            .get_function(scope)
            .unwrap();
        self.set_value(scope, name, func.into());
    }

    fn child(&self, scope: &mut v8::PinScope<'s, '_>, name: &str) -> Local<'s, Object> {
        let child = v8::Object::new(scope);
        self.set_value(scope, name, child.into());
        child
    }
}
