pub const HOST_API_SHIM_PRELUDE: &str = r#"
if (typeof queueMicrotask === "undefined") {
    globalThis.queueMicrotask = function (fn) { Promise.resolve().then(fn); };
}
if (typeof setTimeout === "undefined") {
    globalThis.setTimeout = function (fn) { queueMicrotask(fn); };
    globalThis.clearTimeout = function () {};
}
if (typeof TextEncoder === "undefined") {
    globalThis.TextEncoder = class TextEncoder {
        encode(str) {
            const bytes = [];
            for (let i = 0; i < str.length; i++) {
                let code = str.codePointAt(i);
                if (code > 0xFFFF) i++;
                if (code < 0x80) bytes.push(code);
                else if (code < 0x800) bytes.push(0xC0 | (code >> 6), 0x80 | (code & 0x3F));
                else if (code < 0x10000) bytes.push(0xE0 | (code >> 12), 0x80 | ((code >> 6) & 0x3F), 0x80 | (code & 0x3F));
                else bytes.push(0xF0 | (code >> 18), 0x80 | ((code >> 12) & 0x3F), 0x80 | ((code >> 6) & 0x3F), 0x80 | (code & 0x3F));
            }
            return new Uint8Array(bytes);
        }
    };
}
if (typeof TextDecoder === "undefined") {
    globalThis.TextDecoder = class TextDecoder {
        decode(bytes) {
            let out = "";
            for (let i = 0; i < bytes.length; i++) out += String.fromCharCode(bytes[i]);
            return out;
        }
    };
}
if (typeof MessageChannel === "undefined") {
    class MessagePort {
        constructor() { this.onmessage = null; this._other = null; }
        postMessage(data) {
            const other = this._other;
            queueMicrotask(() => { if (other && other.onmessage) other.onmessage({ data }); });
        }
    }
    globalThis.MessageChannel = class MessageChannel {
        constructor() {
            this.port1 = new MessagePort();
            this.port2 = new MessagePort();
            this.port1._other = this.port2;
            this.port2._other = this.port1;
        }
    };
}
"#;