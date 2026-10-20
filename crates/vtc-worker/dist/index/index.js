import { state } from './snippets/worker-735232a012c24f5a/inline0.js';
import source wasmModule from "./index_bg.wasm";


export class ContainerStartupOptions {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ContainerStartupOptionsFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        __wbg_call_guard();
        wasm.__wbg_containerstartupoptions_free(ptr, 0);

    }
    /**
     * @returns {boolean | undefined}
     */
    get enableInternet() {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        let ret;
        __wbg_call_guard();
        ret = wasm.__wbg_get_containerstartupoptions_enableInternet(this.__wbg_ptr);
        return ret === 0xFFFFFF ? undefined : ret !== 0;
    }
    /**
     * @returns {string[]}
     */
    get entrypoint() {
        try {
            if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
                throw new Error('Invalid stale object from previous Wasm instance');
            }
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            __wbg_call_guard();
            wasm.__wbg_get_containerstartupoptions_entrypoint(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v1 = getArrayJsValueFromWasm0(r0, r1);
            wasm.__wbindgen_export4(r0, r1 * 4, 4);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {Map<any, any>}
     */
    get env() {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        let ret;
        __wbg_call_guard();
        ret = wasm.__wbg_get_containerstartupoptions_env(this.__wbg_ptr);
        return takeObject(ret);
    }
    /**
     * @param {boolean | null} [arg0]
     */
    set enableInternet(arg0) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        __wbg_call_guard();
        wasm.__wbg_set_containerstartupoptions_enableInternet(this.__wbg_ptr, isLikeNone(arg0) ? 0xFFFFFF : arg0 ? 1 : 0);
    }
    /**
     * @param {string[]} arg0
     */
    set entrypoint(arg0) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        const ptr0 = passArrayJsValueToWasm0(arg0, wasm.__wbindgen_export);
        const len0 = WASM_VECTOR_LEN;
        __wbg_call_guard();
        wasm.__wbg_set_containerstartupoptions_entrypoint(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {Map<any, any>} arg0
     */
    set env(arg0) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        __wbg_call_guard();
        wasm.__wbg_set_containerstartupoptions_env(this.__wbg_ptr, addHeapObject(arg0));
    }
}
if (Symbol.dispose) ContainerStartupOptions.prototype[Symbol.dispose] = ContainerStartupOptions.prototype.free;

export class IntoUnderlyingByteSource {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        IntoUnderlyingByteSourceFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        __wbg_call_guard();
        wasm.__wbg_intounderlyingbytesource_free(ptr, 0);

    }
    /**
     * @returns {number}
     */
    get autoAllocateChunkSize() {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        let ret;
        __wbg_call_guard();
        ret = wasm.intounderlyingbytesource_autoAllocateChunkSize(this.__wbg_ptr);
        return ret >>> 0;
    }
    cancel() {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        const ptr = this.__destroy_into_raw();
        __wbg_call_guard();
        wasm.intounderlyingbytesource_cancel(ptr);
    }
    /**
     * @param {ReadableByteStreamController} controller
     * @returns {Promise<any>}
     */
    pull(controller) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        let ret;
        __wbg_call_guard();
        ret = wasm.intounderlyingbytesource_pull(this.__wbg_ptr, addHeapObject(controller));
        return takeObject(ret);
    }
    /**
     * @param {ReadableByteStreamController} controller
     */
    start(controller) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        __wbg_call_guard();
        wasm.intounderlyingbytesource_start(this.__wbg_ptr, addHeapObject(controller));
    }
    /**
     * @returns {ReadableStreamType}
     */
    get type() {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        let ret;
        __wbg_call_guard();
        ret = wasm.intounderlyingbytesource_type(this.__wbg_ptr);
        return __wbindgen_enum_ReadableStreamType[ret];
    }
}
if (Symbol.dispose) IntoUnderlyingByteSource.prototype[Symbol.dispose] = IntoUnderlyingByteSource.prototype.free;

export class IntoUnderlyingSink {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        IntoUnderlyingSinkFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        __wbg_call_guard();
        wasm.__wbg_intounderlyingsink_free(ptr, 0);

    }
    /**
     * @param {any} reason
     * @returns {Promise<any>}
     */
    abort(reason) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        const ptr = this.__destroy_into_raw();
        let ret;
        __wbg_call_guard();
        ret = wasm.intounderlyingsink_abort(ptr, addHeapObject(reason));
        return takeObject(ret);
    }
    /**
     * @returns {Promise<any>}
     */
    close() {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        const ptr = this.__destroy_into_raw();
        let ret;
        __wbg_call_guard();
        ret = wasm.intounderlyingsink_close(ptr);
        return takeObject(ret);
    }
    /**
     * @param {any} chunk
     * @returns {Promise<any>}
     */
    write(chunk) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        let ret;
        __wbg_call_guard();
        ret = wasm.intounderlyingsink_write(this.__wbg_ptr, addHeapObject(chunk));
        return takeObject(ret);
    }
}
if (Symbol.dispose) IntoUnderlyingSink.prototype[Symbol.dispose] = IntoUnderlyingSink.prototype.free;

export class IntoUnderlyingSource {
    static __wrap(ptr) {
        const obj = Object.create(IntoUnderlyingSource.prototype);
        obj.__wbg_ptr = ptr;
        Object.defineProperty(obj, '__wbg_inst', { value: __wbg_instance_id, writable: true });

        IntoUnderlyingSourceFinalization.register(obj, { ptr, instance: __wbg_instance_id }, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        IntoUnderlyingSourceFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        __wbg_call_guard();
        wasm.__wbg_intounderlyingsource_free(ptr, 0);

    }
    cancel() {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        const ptr = this.__destroy_into_raw();
        __wbg_call_guard();
        wasm.intounderlyingsource_cancel(ptr);
    }
    /**
     * @param {ReadableStreamDefaultController} controller
     * @returns {Promise<any>}
     */
    pull(controller) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        let ret;
        __wbg_call_guard();
        ret = wasm.intounderlyingsource_pull(this.__wbg_ptr, addHeapObject(controller));
        return takeObject(ret);
    }
}
if (Symbol.dispose) IntoUnderlyingSource.prototype[Symbol.dispose] = IntoUnderlyingSource.prototype.free;

/**
 * Configuration options for Cloudflare's minification features:
 * <https://www.cloudflare.com/website-optimization/>
 */
export class MinifyConfig {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        MinifyConfigFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        __wbg_call_guard();
        wasm.__wbg_minifyconfig_free(ptr, 0);

    }
    /**
     * @returns {boolean}
     */
    get css() {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        let ret;
        __wbg_call_guard();
        ret = wasm.__wbg_get_minifyconfig_css(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get html() {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        let ret;
        __wbg_call_guard();
        ret = wasm.__wbg_get_minifyconfig_html(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get js() {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        let ret;
        __wbg_call_guard();
        ret = wasm.__wbg_get_minifyconfig_js(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @param {boolean} arg0
     */
    set css(arg0) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        __wbg_call_guard();
        wasm.__wbg_set_minifyconfig_css(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set html(arg0) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        __wbg_call_guard();
        wasm.__wbg_set_minifyconfig_html(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set js(arg0) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        __wbg_call_guard();
        wasm.__wbg_set_minifyconfig_js(this.__wbg_ptr, arg0);
    }
}
if (Symbol.dispose) MinifyConfig.prototype[Symbol.dispose] = MinifyConfig.prototype.free;

export class R2Range {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        R2RangeFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        __wbg_call_guard();
        wasm.__wbg_r2range_free(ptr, 0);

    }
    /**
     * @returns {number | undefined}
     */
    get length() {
        try {
            if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
                throw new Error('Invalid stale object from previous Wasm instance');
            }
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            __wbg_call_guard();
            wasm.__wbg_get_r2range_length(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r2 = getDataViewMemory0().getFloat64(retptr + 8 * 1, true);
            return r0 === 0 ? undefined : r2;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {number | undefined}
     */
    get offset() {
        try {
            if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
                throw new Error('Invalid stale object from previous Wasm instance');
            }
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            __wbg_call_guard();
            wasm.__wbg_get_r2range_offset(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r2 = getDataViewMemory0().getFloat64(retptr + 8 * 1, true);
            return r0 === 0 ? undefined : r2;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {number | undefined}
     */
    get suffix() {
        try {
            if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
                throw new Error('Invalid stale object from previous Wasm instance');
            }
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            __wbg_call_guard();
            wasm.__wbg_get_r2range_suffix(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r2 = getDataViewMemory0().getFloat64(retptr + 8 * 1, true);
            return r0 === 0 ? undefined : r2;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {number | null} [arg0]
     */
    set length(arg0) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        __wbg_call_guard();
        wasm.__wbg_set_r2range_length(this.__wbg_ptr, !isLikeNone(arg0), isLikeNone(arg0) ? 0 : arg0);
    }
    /**
     * @param {number | null} [arg0]
     */
    set offset(arg0) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        __wbg_call_guard();
        wasm.__wbg_set_r2range_offset(this.__wbg_ptr, !isLikeNone(arg0), isLikeNone(arg0) ? 0 : arg0);
    }
    /**
     * @param {number | null} [arg0]
     */
    set suffix(arg0) {
        if (this.__wbg_inst !== undefined && this.__wbg_inst !== __wbg_instance_id) {
            throw new Error('Invalid stale object from previous Wasm instance');
        }
        __wbg_call_guard();
        wasm.__wbg_set_r2range_suffix(this.__wbg_ptr, !isLikeNone(arg0), isLikeNone(arg0) ? 0 : arg0);
    }
}
if (Symbol.dispose) R2Range.prototype[Symbol.dispose] = R2Range.prototype.free;

function __wbg_reset_state () {
    __wbg_instance_id++;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    if (typeof numBytesDecoded !== 'undefined') numBytesDecoded = 0;
    if (typeof WASM_VECTOR_LEN !== 'undefined') WASM_VECTOR_LEN = 0;
    if (typeof heap !== 'undefined') {
        heap = new Array(1024).fill(undefined);
        heap = heap.concat([undefined, null, true, false]);
        if (typeof heap_next !== 'undefined')
        heap_next = heap.length;
    }
    __wbg_reinit_scheduled = false;
    wasmInstance = new WebAssembly.Instance(wasmModule, __wbg_get_imports());
    wasm = wasmInstance.exports;
    wasm.__wbindgen_start();
}

/**
 * @returns {any}
 */
export function __worker_init_state() {
    let ret;
    __wbg_call_guard();
    ret = wasm.__worker_init_state();
    return takeObject(ret);
}

/**
 * @param {Request} req
 * @param {any} env
 * @param {any} ctx
 * @returns {Promise<any>}
 */
export function fetch(req, env, ctx) {
    let ret;
    __wbg_call_guard();
    ret = wasm.fetch(addHeapObject(req), addHeapObject(env), addHeapObject(ctx));
    return takeObject(ret);
}

export function init() {
    __wbg_call_guard();
    wasm.init();
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_67e7344beaa85059: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return addHeapObject(ret);
        },
        __wbg_Number_c54e7112a3fa7e3e: function(arg0) {
            const ret = Number(getObject(arg0));
            return ret;
        },
        __wbg_String_8564e559799eccda: function(arg0, arg1) {
            const ret = String(getObject(arg1));
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_bigint_get_as_i64_b482365c149396c8: function(arg0, arg1) {
            const v = getObject(arg1);
            const ret = typeof(v) === 'bigint' ? v : undefined;
            getDataViewMemory0().setBigInt64(arg0 + 8 * 1, isLikeNone(ret) ? BigInt(0) : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_boolean_get_7a12af2b3f899c5a: function(arg0) {
            const v = getObject(arg0);
            const ret = typeof(v) === 'boolean' ? v : undefined;
            return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
        },
        __wbg___wbindgen_debug_string_0e68cf47c9cbd9b0: function(arg0, arg1) {
            const ret = debugString(getObject(arg1));
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_in_50072d4d6e45c193: function(arg0, arg1) {
            const ret = getObject(arg0) in getObject(arg1);
            return ret;
        },
        __wbg___wbindgen_is_bigint_60fc0336cb14f5d7: function(arg0) {
            const ret = typeof(getObject(arg0)) === 'bigint';
            return ret;
        },
        __wbg___wbindgen_is_function_fcda5e3902d732fe: function(arg0) {
            const ret = typeof(getObject(arg0)) === 'function';
            return ret;
        },
        __wbg___wbindgen_is_null_5160b3e381865372: function(arg0) {
            const ret = getObject(arg0) === null;
            return ret;
        },
        __wbg___wbindgen_is_null_or_undefined_4b0bf0653367120c: function(arg0) {
            const ret = getObject(arg0) == null;
            return ret;
        },
        __wbg___wbindgen_is_object_edb6b15aa3afe12e: function(arg0) {
            const val = getObject(arg0);
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_undefined_8c687d0b90d5b524: function(arg0) {
            const ret = getObject(arg0) === undefined;
            return ret;
        },
        __wbg___wbindgen_jsval_eq_9fdcd3c0a860dd3b: function(arg0, arg1) {
            const ret = getObject(arg0) === getObject(arg1);
            return ret;
        },
        __wbg___wbindgen_jsval_loose_eq_3c30021c243b64cd: function(arg0, arg1) {
            const ret = getObject(arg0) == getObject(arg1);
            return ret;
        },
        __wbg___wbindgen_number_get_1dc732b810cb937c: function(arg0, arg1) {
            const obj = getObject(arg1);
            const ret = typeof(obj) === 'number' ? obj : undefined;
            getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_reinit_0256f5d04898665d: function() {
            __wbg_reinit_scheduled = true;
        },
        __wbg___wbindgen_string_get_92ab86bb19cbc12f: function(arg0, arg1) {
            const obj = getObject(arg1);
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_5d9e815e6fdf150f: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg__wbg_cb_unref_997e73d32238e655: function(arg0) {
            getObject(arg0)._wbg_cb_unref();
        },
        __wbg_arrayBuffer_54a7830fabc87bf7: function() { return handleError(function (arg0) {
            const ret = getObject(arg0).arrayBuffer();
            return addHeapObject(ret);
        }, arguments); },
        __wbg_arrayBuffer_fe1143ba43cd9430: function() { return handleError(function (arg0) {
            const ret = getObject(arg0).arrayBuffer();
            return addHeapObject(ret);
        }, arguments); },
        __wbg_batch_9c73dab7778289bb: function() { return handleError(function (arg0, arg1) {
            const ret = getObject(arg0).batch(takeObject(arg1));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_bind_dfaad7dc9c9785ad: function() { return handleError(function (arg0, arg1) {
            const ret = getObject(arg0).bind(...(takeObject(arg1)));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_buffer_4a989bded7035f57: function(arg0) {
            const ret = getObject(arg0).buffer;
            return addHeapObject(ret);
        },
        __wbg_byobRequest_f161fc37241dd3d4: function(arg0) {
            const ret = getObject(arg0).byobRequest;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_byteLength_0ddb1795e2c7e689: function(arg0) {
            const ret = getObject(arg0).byteLength;
            return ret;
        },
        __wbg_byteOffset_46eb015f52b6ad7d: function(arg0) {
            const ret = getObject(arg0).byteOffset;
            return ret;
        },
        __wbg_call_269c5566fbede3eb: function() { return handleError(function (arg0, arg1) {
            const ret = getObject(arg0).call(getObject(arg1));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_call_6bcf8d3e20937e46: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = getObject(arg0).call(getObject(arg1), getObject(arg2));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_cause_36bd622854f9b3a4: function(arg0) {
            const ret = getObject(arg0).cause;
            return addHeapObject(ret);
        },
        __wbg_cf_c7b0f56b1787c078: function() { return handleError(function (arg0) {
            const ret = getObject(arg0).cf;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        }, arguments); },
        __wbg_close_22882088c136df25: function() { return handleError(function (arg0) {
            getObject(arg0).close();
        }, arguments); },
        __wbg_close_8b609460dd26e367: function() { return handleError(function (arg0) {
            getObject(arg0).close();
        }, arguments); },
        __wbg_constructor_207e5664c25e1af9: function(arg0) {
            const ret = getObject(arg0).constructor;
            return addHeapObject(ret);
        },
        __wbg_done_cffed884d87aa22e: function(arg0) {
            const ret = getObject(arg0).done;
            return ret;
        },
        __wbg_enqueue_6cb545d22db14f33: function() { return handleError(function (arg0, arg1) {
            getObject(arg0).enqueue(getObject(arg1));
        }, arguments); },
        __wbg_entries_972a87586902cf87: function(arg0) {
            const ret = Object.entries(getObject(arg0));
            return addHeapObject(ret);
        },
        __wbg_error_756c5934221e6fee: function(arg0) {
            console.error(getObject(arg0));
        },
        __wbg_error_b7e73e671a58a488: function(arg0, arg1) {
            console.error(getObject(arg0), getObject(arg1));
        },
        __wbg_first_0f1daaa9c0507377: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = getObject(arg0).first(arg1 === 0 ? undefined : getStringFromWasm0(arg1, arg2));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_getRandomValues_973dabfb0ad1769f: function() { return handleError(function (arg0, arg1) {
            globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
        }, arguments); },
        __wbg_getRandomValues_a608c4436c19407a: function() { return handleError(function (arg0, arg1) {
            globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
        }, arguments); },
        __wbg_getTime_65922ba0b59d55a7: function(arg0) {
            const ret = getObject(arg0).getTime();
            return ret;
        },
        __wbg_get_423ea1f7ff5c8d64: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(getObject(arg0), getObject(arg1));
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        }, arguments); },
        __wbg_get_6cf5a4d4d8ad3c5a: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(getObject(arg0), getObject(arg1));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_get_94d18d21679927f6: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg1;
                deferred0_1 = arg2;
                const ret = getObject(arg0).get(getStringFromWasm0(arg1, arg2), takeObject(arg3));
                return addHeapObject(ret);
            } finally {
                __wbg_call_guard();
                wasm.__wbindgen_export4(deferred0_0, deferred0_1, 1);
            }
        }, arguments); },
        __wbg_get_989d0a1309644f2b: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(getObject(arg0), getObject(arg1));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_get_b1f0ab13c737f856: function(arg0, arg1) {
            const ret = getObject(arg0)[arg1 >>> 0];
            return addHeapObject(ret);
        },
        __wbg_get_b5793eadbdf6b0ae: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = getObject(arg1).get(getStringFromWasm0(arg2, arg3));
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_get_unchecked_363572bdd397d473: function(arg0, arg1) {
            const ret = getObject(arg0)[arg1 >>> 0];
            return addHeapObject(ret);
        },
        __wbg_get_with_ref_key_6412cf3094599694: function(arg0, arg1) {
            const ret = getObject(arg0)[getObject(arg1)];
            return addHeapObject(ret);
        },
        __wbg_headers_a1e9854406915ee7: function(arg0) {
            const ret = getObject(arg0).headers;
            return addHeapObject(ret);
        },
        __wbg_instanceId_232fcea31f0d9b32: function(arg0) {
            const ret = getObject(arg0).instanceId;
            return ret;
        },
        __wbg_instanceof_ArrayBuffer_d4ff01f8247925ae: function(arg0) {
            let result;
            try {
                result = getObject(arg0) instanceof ArrayBuffer;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Error_fe6fa771c78ee4cf: function(arg0) {
            let result;
            try {
                result = getObject(arg0) instanceof Error;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Map_1ff6a2b54c899f0d: function(arg0) {
            let result;
            try {
                result = getObject(arg0) instanceof Map;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Uint8Array_598adc0fef426aa8: function(arg0) {
            let result;
            try {
                result = getObject(arg0) instanceof Uint8Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_isArray_5674713bb7b79043: function(arg0) {
            const ret = Array.isArray(getObject(arg0));
            return ret;
        },
        __wbg_isSafeInteger_8f51c743827d1ec5: function(arg0) {
            const ret = Number.isSafeInteger(getObject(arg0));
            return ret;
        },
        __wbg_iterator_22ddeb808cf55a6f: function() {
            const ret = Symbol.iterator;
            return addHeapObject(ret);
        },
        __wbg_json_8f146c95a0d6e4c4: function() { return handleError(function (arg0) {
            const ret = getObject(arg0).json();
            return addHeapObject(ret);
        }, arguments); },
        __wbg_length_31bdaf014f5fbde2: function(arg0) {
            const ret = getObject(arg0).length;
            return ret;
        },
        __wbg_length_4e1adc0d42e23620: function(arg0) {
            const ret = getObject(arg0).length;
            return ret;
        },
        __wbg_message_1cbc5bc03dcf1dee: function(arg0) {
            const ret = getObject(arg0).message;
            return addHeapObject(ret);
        },
        __wbg_method_f625716bf1f7b540: function(arg0, arg1) {
            const ret = getObject(arg1).method;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_name_b4e1ee96e711fbdf: function(arg0) {
            const ret = getObject(arg0).name;
            return addHeapObject(ret);
        },
        __wbg_name_b9d8f2ea16b22045: function(arg0) {
            const ret = getObject(arg0).name;
            return addHeapObject(ret);
        },
        __wbg_new_0_35540e542ba689d2: function() {
            const ret = new Date();
            return addHeapObject(ret);
        },
        __wbg_new_0afe64b4dc16ab74: function() { return handleError(function () {
            const ret = new Headers();
            return addHeapObject(ret);
        }, arguments); },
        __wbg_new_1da3429bc3c4541c: function(arg0) {
            const ret = new Uint8Array(getObject(arg0));
            return addHeapObject(ret);
        },
        __wbg_new_4a37f5fa262c7461: function() { return handleError(function (arg0) {
            const ret = new FixedLengthStream(arg0 >>> 0);
            return addHeapObject(ret);
        }, arguments); },
        __wbg_new_a32a1ab6c6655abe: function(arg0, arg1) {
            const ret = new Error(getStringFromWasm0(arg0, arg1));
            return addHeapObject(ret);
        },
        __wbg_new_bebc3f4757acf305: function() {
            const ret = new Object();
            return addHeapObject(ret);
        },
        __wbg_new_big_int_cba003719ea1046b: function() { return handleError(function (arg0) {
            const ret = new FixedLengthStream(takeObject(arg0));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_new_ffa92086ea89f79c: function() {
            const ret = new Array();
            return addHeapObject(ret);
        },
        __wbg_new_typed_6f8b0d724fe26c07: function(arg0, arg1) {
            try {
                var state0 = {a: arg0, b: arg1};
                var cb0 = (arg0, arg1) => {
                    const a = state0.a;
                    state0.a = 0;
                    try {
                        return __wasm_bindgen_func_elem_2559(a, state0.b, arg0, arg1);
                    } finally {
                        state0.a = a;
                    }
                };
                const ret = new Promise(cb0);
                return addHeapObject(ret);
            } finally {
                state0.a = 0;
            }
        },
        __wbg_new_with_byte_offset_and_length_492c969e8b5da8a4: function(arg0, arg1, arg2) {
            const ret = new Uint8Array(getObject(arg0), arg1 >>> 0, arg2 >>> 0);
            return addHeapObject(ret);
        },
        __wbg_new_with_into_underlying_source_8812003c9985c511: function(arg0, arg1) {
            const ret = new ReadableStream(IntoUnderlyingSource.__wrap(arg0), takeObject(arg1));
            return addHeapObject(ret);
        },
        __wbg_new_with_length_5ffeddb9d9fbb96f: function(arg0) {
            const ret = new Uint8Array(arg0 >>> 0);
            return addHeapObject(ret);
        },
        __wbg_new_with_opt_buffer_source_and_init_935ee9753c594708: function() { return handleError(function (arg0, arg1) {
            const ret = new Response(getObject(arg0), getObject(arg1));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_new_with_opt_readable_stream_and_init_652b1d60607d75aa: function() { return handleError(function (arg0, arg1) {
            const ret = new Response(getObject(arg0), getObject(arg1));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_new_with_opt_str_and_init_cbaf7a52e7dc0b00: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = new Response(arg0 === 0 ? undefined : getStringFromWasm0(arg0, arg1), getObject(arg2));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_next_95053e306b1c3aed: function(arg0) {
            const ret = getObject(arg0).next;
            return addHeapObject(ret);
        },
        __wbg_next_f31ecb8646d2c605: function() { return handleError(function (arg0) {
            const ret = getObject(arg0).next();
            return addHeapObject(ret);
        }, arguments); },
        __wbg_pipeTo_98561dbed8432a42: function(arg0, arg1) {
            const ret = getObject(arg0).pipeTo(getObject(arg1));
            return addHeapObject(ret);
        },
        __wbg_prepare_bf99de14e1a6f936: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = getObject(arg0).prepare(getStringFromWasm0(arg1, arg2));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_prototypesetcall_ae9f5e7459250748: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), getObject(arg2));
        },
        __wbg_push_bfdf956ba476f65b: function(arg0, arg1) {
            const ret = getObject(arg0).push(getObject(arg1));
            return ret;
        },
        __wbg_put_60573b50286940a5: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg1;
                deferred0_1 = arg2;
                const ret = getObject(arg0).put(getStringFromWasm0(arg1, arg2), takeObject(arg3), takeObject(arg4));
                return addHeapObject(ret);
            } finally {
                __wbg_call_guard();
                wasm.__wbindgen_export4(deferred0_0, deferred0_1, 1);
            }
        }, arguments); },
        __wbg_queueMicrotask_85c90f6987555d65: function(arg0) {
            const ret = getObject(arg0).queueMicrotask;
            return addHeapObject(ret);
        },
        __wbg_queueMicrotask_f6a1fa10b81d1fc0: function(arg0) {
            queueMicrotask(getObject(arg0));
        },
        __wbg_raw_4831173a9685345c: function() { return handleError(function (arg0) {
            const ret = getObject(arg0).raw();
            return addHeapObject(ret);
        }, arguments); },
        __wbg_readable_dd742feaf61adcb9: function(arg0) {
            const ret = getObject(arg0).readable;
            return addHeapObject(ret);
        },
        __wbg_resolve_35ec7e0c6af4c82c: function(arg0) {
            const ret = Promise.resolve(getObject(arg0));
            return addHeapObject(ret);
        },
        __wbg_respond_83a71686e927ca32: function() { return handleError(function (arg0, arg1) {
            getObject(arg0).respond(arg1 >>> 0);
        }, arguments); },
        __wbg_run_f1bc61ce2e32f151: function() { return handleError(function (arg0) {
            const ret = getObject(arg0).run();
            return addHeapObject(ret);
        }, arguments); },
        __wbg_set_5f2ad37e5e02dc7b: function(arg0, arg1, arg2) {
            getObject(arg0).set(getArrayU8FromWasm0(arg1, arg2));
        },
        __wbg_set_7923e5ea63b41e6b: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            getObject(arg0).set(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
        }, arguments); },
        __wbg_set_a377297433dfea63: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = Reflect.set(getObject(arg0), getObject(arg1), getObject(arg2));
            return ret;
        }, arguments); },
        __wbg_set_criticalError_e391f6c6a38c6d68: function(arg0, arg1) {
            getObject(arg0).criticalError = arg1 !== 0;
        },
        __wbg_set_headers_dfe6a763facd3dcc: function(arg0, arg1) {
            getObject(arg0).headers = getObject(arg1);
        },
        __wbg_set_high_water_mark_089e3476522036ad: function(arg0, arg1) {
            getObject(arg0).highWaterMark = arg1;
        },
        __wbg_set_instanceId_b0493682e07f9aa4: function(arg0, arg1) {
            getObject(arg0).instanceId = arg1 >>> 0;
        },
        __wbg_set_status_74351f5228412c33: function(arg0, arg1) {
            getObject(arg0).status = arg1;
        },
        __wbg_slice_7afeb833209865e0: function(arg0, arg1) {
            const ret = getObject(arg1).slice();
            const ptr1 = passArrayJsValueToWasm0(ret, wasm.__wbindgen_export);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_static_accessor_GLOBAL_8eb4cd83130a11a0: function() {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_static_accessor_GLOBAL_THIS_1e7044f654e934db: function() {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_static_accessor_INIT_STATE_86fe3c7381036fc1: function() {
            const ret = state;
            return addHeapObject(ret);
        },
        __wbg_static_accessor_SELF_d8b50611246a6d92: function() {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_static_accessor_WINDOW_fd0bc376bf0f8b42: function() {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_text_f72d049e69d3ee60: function() { return handleError(function (arg0) {
            const ret = getObject(arg0).text();
            return addHeapObject(ret);
        }, arguments); },
        __wbg_then_7a850dae4493f353: function(arg0, arg1, arg2) {
            const ret = getObject(arg0).then(getObject(arg1), getObject(arg2));
            return addHeapObject(ret);
        },
        __wbg_then_b830475380919203: function(arg0, arg1) {
            const ret = getObject(arg0).then(getObject(arg1));
            return addHeapObject(ret);
        },
        __wbg_url_8d180a4a6996e22d: function(arg0, arg1) {
            const ret = getObject(arg1).url;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_value_c227f843d21da141: function(arg0) {
            const ret = getObject(arg0).value;
            return addHeapObject(ret);
        },
        __wbg_view_d8c7b26e4d4650f1: function(arg0) {
            const ret = getObject(arg0).view;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_writable_bf33d3f9fb228429: function(arg0) {
            const ret = getObject(arg0).writable;
            return addHeapObject(ret);
        },
        __wbindgen_generic_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 429, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, __wasm_bindgen_func_elem_2551);
            return addHeapObject(ret);
        },
        __wbindgen_generic_0000000000000002: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return addHeapObject(ret);
        },
        __wbindgen_generic_0000000000000003: function(arg0) {
            // Cast intrinsic for `I64 -> Externref`.
            const ret = arg0;
            return addHeapObject(ret);
        },
        __wbindgen_generic_0000000000000004: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return addHeapObject(ret);
        },
        __wbindgen_generic_0000000000000005: function(arg0) {
            // Cast intrinsic for `U64 -> Externref`.
            const ret = BigInt.asUintN(64, arg0);
            return addHeapObject(ret);
        },
        __wbindgen_object_clone_ref: function(arg0) {
            const ret = getObject(arg0);
            return addHeapObject(ret);
        },
        __wbindgen_object_drop_ref: function(arg0) {
            takeObject(arg0);
        },
    };
    return {
        __proto__: null,
        "./index_bg.js": import0,
    };
}

function __wbg_call_guard() {
    if (__wbg_reinit_scheduled) {
        __wbg_reset_state();
        return;
    }
}
function __wasm_bindgen_func_elem_2559(arg0, arg1, arg2, arg3) {
    __wbg_call_guard();
    wasm.__wasm_bindgen_func_elem_2559(arg0, arg1, addHeapObject(arg2), addHeapObject(arg3));
}

function __wasm_bindgen_func_elem_2551(arg0, arg1, arg2) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        __wbg_call_guard();
        wasm.__wasm_bindgen_func_elem_2551(retptr, arg0, arg1, addHeapObject(arg2));
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        if (r1) {
            throw takeObject(r0);
        }
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}


const __wbindgen_enum_ReadableStreamType = ["bytes"];


let __wbg_instance_id = 0;
const ContainerStartupOptionsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(({ ptr, instance }) => {
    if (instance === __wbg_instance_id) wasm.__wbg_containerstartupoptions_free(ptr, 1);
});
const IntoUnderlyingByteSourceFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(({ ptr, instance }) => {
    if (instance === __wbg_instance_id) wasm.__wbg_intounderlyingbytesource_free(ptr, 1);
});
const IntoUnderlyingSinkFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(({ ptr, instance }) => {
    if (instance === __wbg_instance_id) wasm.__wbg_intounderlyingsink_free(ptr, 1);
});
const IntoUnderlyingSourceFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(({ ptr, instance }) => {
    if (instance === __wbg_instance_id) wasm.__wbg_intounderlyingsource_free(ptr, 1);
});
const MinifyConfigFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(({ ptr, instance }) => {
    if (instance === __wbg_instance_id) wasm.__wbg_minifyconfig_free(ptr, 1);
});
const R2RangeFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(({ ptr, instance }) => {
    if (instance === __wbg_instance_id) wasm.__wbg_r2range_free(ptr, 1);
});

function addHeapObject(obj) {
    if (heap_next === heap.length) heap.push(heap.length + 1);
    const idx = heap_next;
    heap_next = heap[idx];

    heap[idx] = obj;
    return idx;
}

const CLOSURE_DTORS = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(state => {
    if (state.instance === __wbg_instance_id) {
        wasm.__wbindgen_export5(state.a, state.b);
    }
});

function debugString(val) {
    // primitive types
    const type = typeof val;
    if (type == 'number' || type == 'boolean' || val == null) {
        return  `${val}`;
    }
    if (type == 'string') {
        return `"${val}"`;
    }
    if (type == 'symbol') {
        const description = val.description;
        if (description == null) {
            return 'Symbol';
        } else {
            return `Symbol(${description})`;
        }
    }
    if (type == 'function') {
        const name = val.name;
        if (typeof name == 'string' && name.length > 0) {
            return `Function(${name})`;
        } else {
            return 'Function';
        }
    }
    // objects
    if (Array.isArray(val)) {
        const length = val.length;
        let debug = '[';
        if (length > 0) {
            debug += debugString(val[0]);
        }
        for(let i = 1; i < length; i++) {
            debug += ', ' + debugString(val[i]);
        }
        debug += ']';
        return debug;
    }
    // Test for built-in
    const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
    let className;
    if (builtInMatches && builtInMatches.length > 1) {
        className = builtInMatches[1];
    } else {
        // Failed to match the standard '[object ClassName]'
        return toString.call(val);
    }
    if (className == 'Object') {
        // we're a user defined class or Object
        // JSON.stringify avoids problems with cycles, and is generally much
        // easier than looping through ownProperties of `val`.
        try {
            return 'Object(' + JSON.stringify(val) + ')';
        } catch (_) {
            return 'Object';
        }
    }
    // errors
    if (val instanceof Error) {
        return `${val.name}: ${val.message}\n${val.stack}`;
    }
    // TODO we could test for more things here, like `Set`s and `Map`s.
    return className;
}

function dropObject(idx) {
    if (idx < 1028) return;
    heap[idx] = heap_next;
    heap_next = idx;
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(takeObject(mem.getUint32(i, true)));
    }
    return result;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function getObject(idx) { return heap[idx]; }

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        wasm.__wbindgen_export3(addHeapObject(e));
    }
}

let heap = new Array(1024).fill(undefined);
heap.push(undefined, null, true, false);

let heap_next = heap.length;

function isLikeNone(x) {
    return x === undefined || x === null;
}

function makeMutClosure(arg0, arg1, f) {
    const state = { a: arg0, b: arg1, cnt: 1, instance: __wbg_instance_id };
    const real = (...args) => {

        if (state.instance !== __wbg_instance_id) {
            throw new Error('Cannot invoke closure from previous WASM instance');
        }

        // First up with a closure we increment the internal reference
        // count. This ensures that the Rust closure environment won't
        // be deallocated while we're invoking it.
        state.cnt++;
        const a = state.a;
        state.a = 0;
        try {
            return f(a, state.b, ...args);
        } finally {
            state.a = a;
            real._wbg_cb_unref();
        }
    };
    real._wbg_cb_unref = () => {
        if (--state.cnt === 0) {
            wasm.__wbindgen_export5(state.a, state.b);
            state.a = 0;
            CLOSURE_DTORS.unregister(state);
        }
    };
    CLOSURE_DTORS.register(real, state, state);
    return real;
}

function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    const mem = getDataViewMemory0();
    for (let i = 0; i < array.length; i++) {
        mem.setUint32(ptr + 4 * i, addHeapObject(array[i]), true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

let __wbg_reinit_scheduled = false;

function takeObject(idx) {
    const ret = getObject(idx);
    dropObject(idx);
    return ret;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
function decodeText(ptr, len) {
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmInstance = new WebAssembly.Instance(wasmModule, __wbg_get_imports());
let wasm = wasmInstance.exports;
wasm.__wbindgen_start();
