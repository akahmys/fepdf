// Acrobat's AF* helper functions, which a document expects the processor to provide.
//
// These are not in ISO 32000-2 — it names no AF* function — and they are not in the
// file either. A form's calculate action is typically one line, `AFSimple_Calculate(...)`,
// and the body has to come from here. The API they implement is Adobe's, now ISO/DIS
// 21757-1.
//
// Written in JavaScript rather than Rust because that is the language they are specified
// in and the one every other implementation writes them in. What that costs is stated in
// `crates/fepdf-script/src/helpers.rs`: none of RR-15's fifteen checks reads this file.
//
// THE HOST CONTRACT, stated because this file cannot discover it.
//
//   __fepdf_doc__   the document. `getField(name)` returns an object whose `value` is an
//                   accessor — reading queries the document, assigning applies a
//                   SetFormFieldValue operation — or null when the form has no such field.
//   event           the field action's event object. A calculation writes `event.value`;
//                   it does not return.
//
// `this` is NOT the document inside these functions. The script that calls them has the
// document as `this`, but a plain call like `AFSimple_Calculate(...)` does not pass it on
// — in a non-strict function `this` is then globalThis. Reaching for `this.getField` here
// throws "not a callable function", which is what it did before this note existed.
//
// Mozilla's pdf.js carries an implementation of the same API under Apache-2.0
// (`src/scripting_api/aform.js`) and it was read while writing this. It could not be used
// directly: it is an ES module exporting a class whose constructor takes four host
// objects (`document`, `app`, `util`, `color`), and Acrobat exposes these as globals. The
// behaviours below follow the documented API rather than that code.

function AFMakeNumber(value) {
    if (typeof value === "number") { return value; }
    if (typeof value !== "string") { return null; }
    // A comma is a decimal separator in most of the world, and a form filled in one
    // locale is read in another.
    var n = parseFloat(value.trim().replace(",", "."));
    return (isNaN(n) || !isFinite(n)) ? null : n;
}

function AFMakeArrayFromList(list) {
    if (Array.isArray(list)) { return list; }
    if (typeof list !== "string") { return []; }
    return list.split(",").map(function (s) { return s.trim(); });
}

function AFSimple(cFunction, nValue1, nValue2) {
    var a = AFMakeNumber(nValue1);
    var b = AFMakeNumber(nValue2);
    if (a === null || b === null) { throw new Error("Invalid value in AFSimple"); }
    switch (cFunction) {
        case "AVG": return (a + b) / 2;
        case "SUM": return a + b;
        case "PRD": return a * b;
        case "MIN": return Math.min(a, b);
        case "MAX": return Math.max(a, b);
    }
    throw new Error("Invalid cFunction in AFSimple");
}

// The one a form actually calls. It writes `event.value` rather than returning, because
// that is how a calculate action reports its result (12.6.3).
function AFSimple_Calculate(cFunction, cFields) {
    var names = AFMakeArrayFromList(cFields);
    var values = [];
    for (var i = 0; i < names.length; i++) {
        var field = __fepdf_doc__.getField(names[i]);
        if (!field) { continue; }
        var n = AFMakeNumber(field.value);
        values.push(n === null ? 0 : n);
    }
    if (values.length === 0) { event.value = 0; return; }

    var result;
    switch (cFunction) {
        case "SUM": result = values.reduce(function (a, b) { return a + b; }, 0); break;
        case "PRD": result = values.reduce(function (a, b) { return a * b; }, 1); break;
        case "AVG":
            result = values.reduce(function (a, b) { return a + b; }, 0) / values.length;
            break;
        case "MIN": result = Math.min.apply(null, values); break;
        case "MAX": result = Math.max.apply(null, values); break;
        default: throw new TypeError("Invalid function in AFSimple_Calculate");
    }
    // Six places, so that 0.1 + 0.2 is 0.3 in a form rather than 0.30000000000000004.
    event.value = Math.round(1e6 * result) / 1e6;
}
