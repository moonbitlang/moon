// @ts-check
// MoonBit symbol demangler for JS runtime fallback.
// Parsing is split into: mangled string -> AST -> rendered display text.

/**
 * Rough AST type docs (TypeScript-style via JSDoc) for the demangler.
 * These are intentionally lightweight and focus on the parse/render boundary.
 */

/**
 * A parsed type path such as `@pkg.Type` or `@Type`.
 * @typedef {{
 *   kind: "TypePath",
 *   pkg: string,
 *   typeName: string,
 * }} TypePathAst
 */

/** @typedef {{
 *   kind: "Function",
 *   pkg: string,
 *   name: string,
 *   nested: string[],
 *   anonIndex: string | null,
 *   typeArgs: string | null,
 * }} FunctionSymbolAst */

/** @typedef {{
 *   kind: "Method",
 *   pkg: string,
 *   typeName: string,
 *   methodName: string,
 *   typeArgs: string | null,
 * }} MethodSymbolAst */

/** @typedef {{
 *   kind: "TraitImplMethod",
 *   implType: TypePathAst,
 *   traitType: TypePathAst,
 *   methodName: string,
 *   typeArgs: string | null,
 * }} TraitImplMethodSymbolAst */

/** @typedef {{
 *   kind: "ExtensionMethod",
 *   typePkg: string,
 *   typeName: string,
 *   methodPkg: string,
 *   methodName: string,
 *   typeArgs: string | null,
 * }} ExtensionMethodSymbolAst */

/** @typedef {{
 *   kind: "Type",
 *   typePath: TypePathAst,
 * }} TypeSymbolAst */

/** @typedef {{
 *   kind: "Local",
 *   ident: string,
 *   stamp: string,
 * }} LocalSymbolAst */

/**
 * Parsed top-level symbol.
 * @typedef {FunctionSymbolAst
 *   | MethodSymbolAst
 *   | TraitImplMethodSymbolAst
 *   | ExtensionMethodSymbolAst
 *   | TypeSymbolAst
 *   | LocalSymbolAst} SymbolAst
 */

/** @typedef {[number, number]} U32ParseResult */
/** @typedef {[string, number]} StringParseResult */
/** @typedef {[string[], number]} StringListParseResult */
/** @typedef {[string | null, number]} OptionalStringParseResult */
/** @typedef {[TypePathAst, number]} TypePathParseResult */
/** @typedef {[FunctionSymbolAst, number]} FunctionSymbolParseResult */
/** @typedef {[MethodSymbolAst, number]} MethodSymbolParseResult */
/** @typedef {[TraitImplMethodSymbolAst, number]} TraitImplMethodSymbolParseResult */
/** @typedef {[ExtensionMethodSymbolAst, number]} ExtensionMethodSymbolParseResult */
/** @typedef {[TypeSymbolAst, number]} TypeSymbolParseResult */
/** @typedef {[LocalSymbolAst, number]} LocalSymbolParseResult */
/** @typedef {[SymbolAst, number]} SymbolParseResult */

/** @returns {Error} */
function moonbitDemangleError() {
    return new Error("moonbit demangle parse failure");
}

/** @param {string} s @param {number} i @returns {string} */
function moonbitMatchDigitsAt(s, i) {
    const matched = /^[0-9]+/.exec(s.slice(i));
    if (!matched) {
        throw moonbitDemangleError();
    }
    return matched[0];
}

/** @param {number} code @returns {number} */
function moonbitHexValue(code) {
    const value = Number.parseInt(String.fromCharCode(code), 16);
    return Number.isNaN(value) ? -1 : value;
}

/** @param {string} s @param {number} i @returns {U32ParseResult} */
function moonbitParseU32(s, i) {
    const digits = moonbitMatchDigitsAt(s, i);
    const value = Number.parseInt(digits, 10);
    if (!Number.isFinite(value) || value > 0xffffffff) {
        throw moonbitDemangleError();
    }
    return [value, i + digits.length];
}

/** @param {string} raw @returns {string} */
function moonbitDecodeIdentifier(raw) {
    let out = "";
    let i = 0;
    while (i < raw.length) {
        const ch = raw[i];
        if (ch !== "_") {
            out += ch;
            i += 1;
            continue;
        }

        if (i + 1 >= raw.length) {
            throw moonbitDemangleError();
        }
        const next = raw[i + 1];
        if (next === "_") {
            out += "_";
            i += 2;
            continue;
        }
        if (i + 2 >= raw.length) {
            throw moonbitDemangleError();
        }

        const hi = moonbitHexValue(raw.charCodeAt(i + 1));
        const lo = moonbitHexValue(raw.charCodeAt(i + 2));
        if (hi < 0 || lo < 0) {
            throw moonbitDemangleError();
        }
        out += String.fromCharCode((hi << 4) | lo);
        i += 3;
    }
    return out;
}

/** @param {string} s @param {number} i @returns {StringParseResult} */
function moonbitParseIdentifier(s, i) {
    const parsed = moonbitParseU32(s, i);
    const n = parsed[0];
    const start = parsed[1];
    const end = start + n;
    if (end > s.length) {
        throw moonbitDemangleError();
    }
    const raw = s.slice(start, end);
    return [moonbitDecodeIdentifier(raw), end];
}

/** @param {string} s @param {number} i @param {number} count @returns {StringParseResult} */
function moonbitParsePackageSegments(s, i, count) {
    const segs = [];
    let j = i;
    for (let k = 0; k < count; k++) {
        const parsed = moonbitParseIdentifier(s, j);
        segs.push(parsed[0]);
        j = parsed[1];
    }
    return [segs.join("/"), j];
}

/** @param {string} s @param {number} i @returns {StringParseResult} */
function moonbitParseCountedPackageSegments(s, i) {
    const parsed = moonbitParseU32(s, i);
    const count = parsed[0];
    const j = parsed[1];
    try {
        return moonbitParsePackageSegments(s, j, count);
    } catch (_) {
        const fallback = Number(moonbitMatchDigitsAt(s, i).slice(0, 1));
        return moonbitParsePackageSegments(s, i + 1, fallback);
    }
}

/** @param {string} s @param {number} i @returns {StringParseResult} */
function moonbitParsePackage(s, i) {
    if (s[i] !== "P") {
        throw moonbitDemangleError();
    }
    let j = i + 1;
    if (s[j] === "B") {
        return ["moonbitlang/core/builtin", j + 1];
    }
    if (s[j] === "C") {
        const parsed = moonbitParseCountedPackageSegments(s, j + 1);
        const suffix = parsed[0];
        const end = parsed[1];
        if (suffix.length === 0) {
            return ["moonbitlang/core", end];
        }
        return [`moonbitlang/core/${suffix}`, end];
    }
    return moonbitParseCountedPackageSegments(s, j);
}

/** @param {string} pkg @returns {boolean} */
function moonbitIsCorePackage(pkg) {
    return /^moonbitlang\/core(?:\/|$)/.test(pkg);
}

/** @param {string} s @param {number} i @param {boolean} omitCorePrefix @returns {TypePathParseResult} */
function moonbitParseTypePath(s, i, omitCorePrefix) {
    const pkgParsed = moonbitParsePackage(s, i);
    let pkg = pkgParsed[0];
    let j = pkgParsed[1];

    const typeParsed = moonbitParseIdentifier(s, j);
    let typeName = typeParsed[0];
    j = typeParsed[1];

    if (s[j] === "L") {
        const localParsed = moonbitParseIdentifier(s, j + 1);
        typeName = `${typeName}.${localParsed[0]}`;
        j = localParsed[1];
    }

    if (omitCorePrefix && moonbitIsCorePackage(pkg)) {
        pkg = "";
    }

    return [{ kind: "TypePath", pkg, typeName }, j];
}

/** @param {string} s @param {number} i @returns {StringListParseResult} */
function moonbitParseTypeTextListUntilE(s, i) {
    const items = [];
    let j = i;
    while (s[j] !== "E") {
        const parsed = moonbitParseTypeText(s, j);
        items.push(parsed[0]);
        j = parsed[1];
    }
    return [items, j + 1];
}

/** @param {string} s @param {number} i @returns {StringParseResult} */
function moonbitParseTypeArgsText(s, i) {
    let j = i;
    let argsPrefix = "";
    if (s[j] === "G") {
        const parsed = moonbitParseTypeTextListUntilE(s, j + 1);
        argsPrefix = `[${parsed[0].join(", ")}]`;
        j = parsed[1];
    }

    let raiseSuffix = "";
    if (s[j] === "H") {
        const parsed = moonbitParseTypeText(s, j + 1);
        raiseSuffix = ` raise ${parsed[0]}`;
        j = parsed[1];
    }
    return [`${argsPrefix}${raiseSuffix}`, j];
}

/** @param {string} s @param {number} i @returns {OptionalStringParseResult} */
function moonbitParseOptionalTypeArgsText(s, i) {
    if (s[i] === "G" || s[i] === "H") {
        const parsed = moonbitParseTypeArgsText(s, i);
        return [parsed[0], parsed[1]];
    }
    return [null, i];
}

/** @param {string} s @param {number} i @returns {StringParseResult} */
function moonbitParseTypeRefText(s, i) {
    if (s[i] !== "R") {
        throw moonbitDemangleError();
    }
    const pathParsed = moonbitParseTypePath(s, i + 1, false);
    let text = moonbitRenderTypePath(pathParsed[0]);
    let j = pathParsed[1];
    if (s[j] === "G") {
        const parsed = moonbitParseTypeArgsText(s, j);
        text += parsed[0];
        j = parsed[1];
    }
    return [text, j];
}

/** @param {string} s @param {number} i @param {boolean} asyncMark @returns {StringParseResult} */
function moonbitParseFnTypeText(s, i, asyncMark) {
    if (s[i] !== "W") {
        throw moonbitDemangleError();
    }
    const paramsParsed = moonbitParseTypeTextListUntilE(s, i + 1);
    const params = paramsParsed[0];
    let j = paramsParsed[1];

    const retParsed = moonbitParseTypeText(s, j);
    const ret = retParsed[0];
    j = retParsed[1];

    let raises = "";
    if (s[j] === "Q") {
        const parsed = moonbitParseTypeText(s, j + 1);
        raises = ` raise ${parsed[0]}`;
        j = parsed[1];
    }

    const prefix = asyncMark ? "async " : "";
    return [`${prefix}(${params.join(", ")}) -> ${ret}${raises}`, j];
}

/** @param {string} s @param {number} i @returns {StringParseResult} */
function moonbitParseTypeText(s, i) {
    const ch = s[i];
    switch (ch) {
        case "i":
            return ["Int", i + 1];
        case "l":
            return ["Int64", i + 1];
        case "h":
            return ["Int16", i + 1];
        case "j":
            return ["UInt", i + 1];
        case "k":
            return ["UInt16", i + 1];
        case "m":
            return ["UInt64", i + 1];
        case "d":
            return ["Double", i + 1];
        case "f":
            return ["Float", i + 1];
        case "b":
            return ["Bool", i + 1];
        case "c":
            return ["Char", i + 1];
        case "s":
            return ["String", i + 1];
        case "u":
            return ["Unit", i + 1];
        case "y":
            return ["Byte", i + 1];
        case "z":
            return ["Bytes", i + 1];
        case "A": {
            const parsed = moonbitParseTypeText(s, i + 1);
            return [`FixedArray[${parsed[0]}]`, parsed[1]];
        }
        case "O": {
            const parsed = moonbitParseTypeText(s, i + 1);
            return [`Option[${parsed[0]}]`, parsed[1]];
        }
        case "U": {
            const parsed = moonbitParseTypeTextListUntilE(s, i + 1);
            return [`(${parsed[0].join(", ")})`, parsed[1]];
        }
        case "V":
            return moonbitParseFnTypeText(s, i + 1, true);
        case "W":
            return moonbitParseFnTypeText(s, i, false);
        case "R":
            return moonbitParseTypeRefText(s, i);
        default:
            throw moonbitDemangleError();
    }
}

/** @param {string} s @param {number} i @returns {StringParseResult} */
function moonbitParseDigits(s, i) {
    const digits = moonbitMatchDigitsAt(s, i);
    return [digits, i + digits.length];
}

/** @param {string} s @param {number} i @returns {FunctionSymbolParseResult} */
function moonbitParseFunctionSymbol(s, i) {
    const pkgParsed = moonbitParsePackage(s, i);
    const pkg = pkgParsed[0];
    let j = pkgParsed[1];

    const nameParsed = moonbitParseIdentifier(s, j);
    const name = nameParsed[0];
    j = nameParsed[1];

    const nested = [];
    while (s[j] === "N") {
        const parsed = moonbitParseIdentifier(s, j + 1);
        nested.push(parsed[0]);
        j = parsed[1];
    }

    let anonIndex = null;
    if (s[j] === "C") {
        const parsed = moonbitParseDigits(s, j + 1);
        anonIndex = parsed[0];
        j = parsed[1];
    }

    const argsParsed = moonbitParseOptionalTypeArgsText(s, j);
    const typeArgs = argsParsed[0];
    j = argsParsed[1];

    return [{ kind: "Function", pkg, name, nested, anonIndex, typeArgs }, j];
}

/** @param {string} s @param {number} i @returns {MethodSymbolParseResult} */
function moonbitParseMethodSymbol(s, i) {
    const pkgParsed = moonbitParsePackage(s, i);
    const pkg = pkgParsed[0];
    let j = pkgParsed[1];

    const typeParsed = moonbitParseIdentifier(s, j);
    const typeName = typeParsed[0];
    j = typeParsed[1];

    const methodParsed = moonbitParseIdentifier(s, j);
    const methodName = methodParsed[0];
    j = methodParsed[1];

    const argsParsed = moonbitParseOptionalTypeArgsText(s, j);
    const typeArgs = argsParsed[0];
    j = argsParsed[1];

    return [{ kind: "Method", pkg, typeName, methodName, typeArgs }, j];
}

/** @param {string} s @param {number} i @returns {TraitImplMethodSymbolParseResult} */
function moonbitParseTraitImplMethodSymbol(s, i) {
    const implParsed = moonbitParseTypePath(s, i, false);
    const implType = implParsed[0];
    let j = implParsed[1];

    const traitParsed = moonbitParseTypePath(s, j, false);
    const traitType = traitParsed[0];
    j = traitParsed[1];

    const methodParsed = moonbitParseIdentifier(s, j);
    const methodName = methodParsed[0];
    j = methodParsed[1];

    const argsParsed = moonbitParseOptionalTypeArgsText(s, j);
    const typeArgs = argsParsed[0];
    j = argsParsed[1];

    return [{ kind: "TraitImplMethod", implType, traitType, methodName, typeArgs }, j];
}

/** @param {string} s @param {number} i @returns {ExtensionMethodSymbolParseResult} */
function moonbitParseExtensionMethodSymbol(s, i) {
    const typePkgParsed = moonbitParsePackage(s, i);
    const typePkg = typePkgParsed[0];
    let j = typePkgParsed[1];

    const typeParsed = moonbitParseIdentifier(s, j);
    const typeName = typeParsed[0];
    j = typeParsed[1];

    const methodPkgParsed = moonbitParsePackage(s, j);
    const methodPkg = methodPkgParsed[0];
    j = methodPkgParsed[1];

    const methodParsed = moonbitParseIdentifier(s, j);
    const methodName = methodParsed[0];
    j = methodParsed[1];

    const argsParsed = moonbitParseOptionalTypeArgsText(s, j);
    const typeArgs = argsParsed[0];
    j = argsParsed[1];

    return [{ kind: "ExtensionMethod", typePkg, typeName, methodPkg, methodName, typeArgs }, j];
}

/** @param {string} s @param {number} i @returns {TypeSymbolParseResult} */
function moonbitParseTypeSymbol(s, i) {
    const parsed = moonbitParseTypePath(s, i, false);
    return [{ kind: "Type", typePath: parsed[0] }, parsed[1]];
}

/** @param {string} s @param {number} i @returns {LocalSymbolParseResult} */
function moonbitParseLocalSymbol(s, i) {
    let j = i;
    if (s[j] === "m") {
        j += 1;
    }

    const identParsed = moonbitParseIdentifier(s, j);
    const ident = identParsed[0];
    j = identParsed[1];

    if (s[j] !== "S") {
        throw moonbitDemangleError();
    }
    const stampParsed = moonbitParseDigits(s, j + 1);
    const stamp = stampParsed[0];
    j = stampParsed[1];

    return [{ kind: "Local", ident, stamp }, j];
}

/** @param {string} funcName @returns {SymbolAst} */
function moonbitParseMangledSymbol(funcName) {
    const prefix = /^\$?_M0/.exec(funcName);
    if (!prefix) {
        throw moonbitDemangleError();
    }
    let i = prefix[0].length;
    if (i >= funcName.length) {
        throw moonbitDemangleError();
    }

    const tag = funcName[i];
    i += 1;

    /** @type {SymbolParseResult} */
    const parsed = (() => {
        switch (tag) {
            case "F":
                return moonbitParseFunctionSymbol(funcName, i);
            case "M":
                return moonbitParseMethodSymbol(funcName, i);
            case "I":
                return moonbitParseTraitImplMethodSymbol(funcName, i);
            case "E":
                return moonbitParseExtensionMethodSymbol(funcName, i);
            case "T":
                return moonbitParseTypeSymbol(funcName, i);
            case "L":
                return moonbitParseLocalSymbol(funcName, i);
            default:
                throw moonbitDemangleError();
        }
    })();

    const symbol = parsed[0];
    const j = parsed[1];
    if (j < funcName.length) {
        const c = funcName[j];
        if (c !== "." && c !== "$" && c !== "@") {
            throw moonbitDemangleError();
        }
    }
    return symbol;
}

/** @param {TypePathAst} path @returns {string} */
function moonbitRenderTypePath(path) {
    return `@${moonbitDotPrefix(path.pkg)}${path.typeName}`;
}

/** @param {string} text @returns {string} */
function moonbitDotPrefix(text) {
    return text.length === 0 ? "" : `${text}.`;
}

/**
 * @param {SymbolAst} symbol
 * @returns {string}
 */
function moonbitRenderDemangledSymbol(symbol) {
    switch (symbol.kind) {
        case "Function": {
            const nested = symbol.nested.length === 0 ? "" : `.${symbol.nested.join(".")}`;
            const anon =
                symbol.anonIndex === null
                    ? ""
                    : `.${symbol.anonIndex} (the ${symbol.anonIndex}-th anonymous-function)`;
            const args = symbol.typeArgs ? symbol.typeArgs : "";
            return `@${moonbitDotPrefix(symbol.pkg)}${symbol.name}${nested}${anon}${args}`;
        }
        case "Method": {
            const args = symbol.typeArgs ? symbol.typeArgs : "";
            return `@${moonbitDotPrefix(symbol.pkg)}${symbol.typeName}::${symbol.methodName}${args}`;
        }
        case "TraitImplMethod": {
            const args = symbol.typeArgs ? symbol.typeArgs : "";
            return `impl ${moonbitRenderTypePath(symbol.traitType)} for ${moonbitRenderTypePath(symbol.implType)}${args} with ${symbol.methodName}`;
        }
        case "ExtensionMethod": {
            const typePkgUse = moonbitIsCorePackage(symbol.typePkg) ? "" : symbol.typePkg;
            const args = symbol.typeArgs ? symbol.typeArgs : "";
            return `@${moonbitDotPrefix(symbol.methodPkg)}${moonbitDotPrefix(typePkgUse)}${symbol.typeName}::${symbol.methodName}${args}`;
        }
        case "Type":
            return moonbitRenderTypePath(symbol.typePath);
        case "Local": {
            const noDollar = symbol.ident.replace(/^\$/, "");
            const shown = noDollar.replace(/\.fn$/, "");
            return `${shown}/${symbol.stamp}`;
        }
        default:
            throw moonbitDemangleError();
    }
}

/**
 * Demangle a MoonBit mangled symbol, or return the original string on failure.
 * @param {string} funcName
 * @returns {string}
 */
function __moonbit_demangle_mangled_function_name(funcName) {
    if (typeof funcName !== "string" || funcName.length === 0) {
        return funcName;
    }
    try {
        const ast = moonbitParseMangledSymbol(funcName);
        return moonbitRenderDemangledSymbol(ast);
    } catch (_) {
        return funcName;
    }
}
