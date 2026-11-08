/* Ordinary B guest. No libc/WASI or Host paragraph algorithm.
 * clang21 AST pretty-printed for review; formatting verified byte-identical object.
 * Source/input/cache chunk size128; scan/output work quantum64 scalars per VM. */
typedef unsigned int u32;
typedef unsigned long long u64;
typedef unsigned char u8;
__attribute__((import_module("qusor"))) __attribute__((import_name("call"))) extern int host_call(int, int, int, int);
static char ctx[4096];
static char meta[4096];
static char reply[4096];
static char request[16384];
static char output[4096];
static u32 request_n;
static u32 output_n;
static int failed;
void *memcpy(void *d, const void *s, unsigned long n) {
    u8 *a = d;
    const u8 *b = s;
    for (unsigned long i = 0; i < n; i++)
        a[i] = b[i];
    return d;
}
void *memset(void *d, int c, unsigned long n) {
    u8 *a = d;
    for (unsigned long i = 0; i < n; i++)
        a[i] = (u8)c;
    return d;
}
typedef struct {
    const char *p;
    u32 n;
} Span;
static u32 len(const char *s) {
    u32 n = 0;
    while (s[n])
        n++;
    return n;
}
static int equal(Span s, const char *t) {
    u32 n = len(t);
    if (n != s.n)
        return 0;
    for (u32 i = 0; i < n; i++)
        if (s.p[i] != t[i])
            return 0;
    return 1;
}
/* Bounded JSON spans: only typed broker replies, no DOM or arbitrary object model. */
static const char *ws(const char *p, const char *e) {
    while (p < e && (u8)*p <= 32)
        p++;
    return p;
}
static const char *endval(const char *p, const char *e) {
    u32 depth = 0, quoted = 0;
    while (p < e)
        {
            char c = *p;
            if (quoted) {
                if (c == '\\') {
                    p += 2;
                    continue;
                }
                if (c == '"') {
                    quoted = 0;
                    if (!depth)
                        return p + 1;
                }
            } else if (c == '"')
                quoted = 1;
            else if (c == '{' || c == '[')
                depth++;
            else if (c == '}' || c == ']') {
                if (!depth)
                    return p;
                if (!--depth)
                    return p + 1;
            } else if (!depth && (c == ',' || (u8)c <= 32))
                return p;
            p++;
        }
    failed = 1;
    return e;
}
static Span field(Span obj, const char *key) {
    const char *p = obj.p, *e = p + obj.n;
    Span none = {0, 0};
    if (p >= e || *p != '{') {
        failed = 1;
        return none;
    }
    p++;
    while (p < e)
        {
            p = ws(p, e);
            const char *k = p, *ke = endval(p, e);
            p = ws(ke, e);
            if (p >= e || *p++ != ':') {
                failed = 1;
                return none;
            }
            p = ws(p, e);
            const char *ve = endval(p, e);
            if (ke - k == (long)len(key) + 2 && k[0] == '"') {
                u32 i = 0;
                while (key[i] && key[i] == k[i + 1])
                    i++;
                if (!key[i]) {
                    Span v = {p, (u32)(ve - p)};
                    return v;
                }
            }
            p = ws(ve, e);
            if (p >= e || *p++ != ',')
                break;
        }
    failed = 1;
    return none;
}
static u64 number(Span s) {
    u64 v = 0;
    if (!s.n || s.n > 20) {
        failed = 1;
        return 0;
    }
    for (u32 i = 0; i < s.n; i++) {
        u32 c = (u8)s.p[i] - 48;
        if (c > 9 || v > (~(u64)0 - c) / 10) {
            failed = 1;
            return 0;
        }
        v = v * 10 + c;
    }
    return v;
}
static void put(const char *p, u32 n) {
    if (n > sizeof (request) - request_n) {
        failed = 1;
        return;
    }
    memcpy(request + request_n, p, n);
    request_n += n;
}
static void lit(const char *s) {
    put(s, len(s));
}
static void span(Span s) {
    put(s.p, s.n);
}
static void decimal(u64 n) {
    char b[20];
    u32 k = 0;
    do {
        b[k++] = (char)('0' + n % 10);
        n /= 10;
    } while (n);
    while (k)
        put(&b[--k], 1);
}
static Span rpc(char *buf) {
    Span v = {buf, 0};
    if (failed)
        return v;
    int n = host_call((int)request, request_n, (int)buf, 4096);
    if (n < 0 || n > 4096) {
        failed = 1;
        return v;
    }
    v.n = (u32)n;
    return v;
}
static Span context;
static u64 seq;
static u64 base;
static u64 checkpoint_revision;
static void identity(void) {
    request_n = 0;
    lit("{\"api_version\":{\"major\":0,\"minor\":3},\"continuation_protocol\":1,\"request_id\":\"paragraph\",\"resource\":");
    span(field(context, "resource"));
    lit(",\"mount_id\":");
    span(field(context, "mount_id"));
    lit(",\"sequence\":");
    decimal(seq);
    lit(",\"base_revision\":");
    decimal(base);
}
/* Canonical standard base64 and UTF-8 JSON string decoding. */
static const char alphabet[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
static void b64encode(const u8 *p, u32 n) {
    for (u32 i = 0; i < n; i += 3) {
        u32 x = (u32)p[i] << 16;
        if (i + 1 < n)
            x |= (u32)p[i + 1] << 8;
        if (i + 2 < n)
            x |= p[i + 2];
        char b[4] = {alphabet[x >> 18], alphabet[(x >> 12) & 63], i + 1 < n ? alphabet[(x >> 6) & 63] : '=', i + 2 < n ? alphabet[x & 63] : '='};
        put(b, 4);
    }
}
static int b64digit(char c) {
    if (c >= 'A' && c <= 'Z')
        return c - 'A';
    if (c >= 'a' && c <= 'z')
        return c - 'a' + 26;
    if (c >= '0' && c <= '9')
        return c - '0' + 52;
    if (c == '+')
        return 62;
    if (c == '/')
        return 63;
    return -1;
}
static u32 b64decode(Span s, u8 *out, u32 cap) {
    if (s.n < 2 || s.p[0] != '"' || s.p[s.n - 1] != '"') {
        failed = 1;
        return 0;
    }
    u32 n = s.n - 2, k = 0;
    if (n % 4) {
        failed = 1;
        return 0;
    }
    for (u32 i = 0; i < n; i += 4) {
        u32 x = 0;
        int padding = 0;
        for (int j = 0; j < 4; j++) {
            char c = s.p[1 + i + j];
            int d = b64digit(c);
            if (c == '=') {
                if (i + 4 != n || j < 2) {
                    failed = 1;
                    return 0;
                }
                padding++;
                d = 0;
            } else if (padding) {
                failed = 1;
                return 0;
            }
            if (d < 0) {
                failed = 1;
                return 0;
            }
            x = (x << 6) | (u32)d;
        }
        if (padding > 2 || (padding == 2 && (x & 65535)) || (padding == 1 && (x & 255))) {
            failed = 1;
            return 0;
        }
        for (int j = 0; j < 3 - padding; j++) {
            if (k >= cap) {
                failed = 1;
                return 0;
            }
            out[k++] = (u8)(x >> (16 - j * 8));
        }
    }
    return k;
}
static int hex(char c) {
    if (c >= '0' && c <= '9')
        return c - '0';
    if (c >= 'a' && c <= 'f')
        return c - 'a' + 10;
    if (c >= 'A' && c <= 'F')
        return c - 'A' + 10;
    failed = 1;
    return 0;
}
static u32 unicode4(const char *p) {
    u32 v = 0;
    for (int i = 0; i < 4; i++)
        v = (v << 4) | (u32)hex(p[i]);
    return v;
}
static u32 unquote(Span s, u8 *out, u32 cap) {
    u32 k = 0;
    if (s.n < 2 || s.p[0] != '"' || s.p[s.n - 1] != '"') {
        failed = 1;
        return 0;
    }
    for (u32 i = 1; i + 1 < s.n; i++) {
        u32 c = (u8)s.p[i];
        if (c == '\\') {
            if (++i + 1 >= s.n) {
                failed = 1;
                return 0;
            }
            c = (u8)s.p[i];
            if (c == 'u') {
                if (i + 4 >= s.n) {
                    failed = 1;
                    return 0;
                }
                c = unicode4(s.p + i + 1);
                i += 4;
                if (c >= 55296 && c <= 56319) {
                    if (i + 6 >= s.n || s.p[i + 1] != '\\' || s.p[i + 2] != 'u') {
                        failed = 1;
                        return 0;
                    }
                    u32 low = unicode4(s.p + i + 3);
                    i += 6;
                    if (low < 56320 || low > 57343) {
                        failed = 1;
                        return 0;
                    }
                    c = 65536 + ((c - 55296) << 10) + (low - 56320);
                }
                u8 b[4];
                u32 z;
                if (c < 128) {
                    b[0] = (u8)c;
                    z = 1;
                } else if (c < 2048) {
                    b[0] = 192 | (c >> 6);
                    b[1] = 128 | (c & 63);
                    z = 2;
                } else if (c < 65536) {
                    b[0] = 224 | (c >> 12);
                    b[1] = 128 | ((c >> 6) & 63);
                    b[2] = 128 | (c & 63);
                    z = 3;
                } else {
                    b[0] = 240 | (c >> 18);
                    b[1] = 128 | ((c >> 12) & 63);
                    b[2] = 128 | ((c >> 6) & 63);
                    b[3] = 128 | (c & 63);
                    z = 4;
                }
                if (z > cap - k) {
                    failed = 1;
                    return 0;
                }
                memcpy(out + k, b, z);
                k += z;
                continue;
            }
            if (c == 'n')
                c = 10;
            else if (c == 'r')
                c = 13;
            else if (c == 't')
                c = 9;
            else if (c == 'b')
                c = 8;
            else if (c == 'f')
                c = 12;
            else if (c != '"' && c != '\\' && c != '/') {
                failed = 1;
                return 0;
            }
        }
        if (k >= cap) {
            failed = 1;
            return 0;
        }
        out[k++] = (u8)c;
    }
    return k;
}
static void quote_bytes(const u8 *p, u32 n) {
    lit("\"");
    for (u32 i = 0; i < n; i++) {
        u8 c = p[i];
        if (c == '"' || c == '\\') {
            lit("\\");
            put((const char *)&c, 1);
        } else if (c < 32) {
            const char h[] = "0123456789abcdef";
            char b[6] = {'\\', 'u', '0', '0', h[c >> 4], h[c & 15]};
            put(b, 6);
        } else
            put((const char *)&c, 1);
    }
    lit("\"");
}
typedef struct {
    u32 pos;
    u32 start;
    u32 n;
    u8 data[128];
} Cursor;
typedef struct {
    u32 magic;
    u32 phase;
    u32 action;
    u32 total;
    u32 input_bytes;
    u32 from;
    u32 to;
    u32 count;
    u32 scan_index;
    u32 body_start;
    u32 previous_cr;
    u32 selected_start;
    u32 selected_end;
    u32 selected_next;
    u32 selected_found;
    u32 target_separator;
    u32 previous_separator;
    u32 previous_body_cr;
    u32 previous_body_empty;
    u32 selected_cr;
    u32 tail_bytes;
    u32 last_empty;
    u32 penultimate_empty;
    u32 previous_previous_cr;
    u32 started;
    u32 ambiguous;
    u32 normal_index;
    u32 out_index;
    u32 part;
    u32 output_offset;
    u32 copy_offset;
    Cursor main;
    Cursor moved;
    Cursor separator;
} Checkpoint;
static Checkpoint cp;
static u8 packed[2048];
/* Version1 zero-run/literal codec. Literal runs include short zero runs;
 * worst-case storage is raw + ceil(raw/128) + three header bytes. */
static u32 pack_checkpoint(void) {
    const u8 *p = (const u8 *)&cp;
    u32 n = sizeof (cp), i = 0, k = 3;
    packed[0] = 1;
    packed[1] = (u8)n;
    packed[2] = (u8)(n >> 8);
    while (i < n)
        {
            u32 zero = 0;
            while (zero < 128 && i + zero < n && !p[i + zero])
                zero++;
            if (zero >= 3) {
                if (k >= sizeof (packed)) {
                    failed = 1;
                    return 0;
                }
                packed[k++] = (u8)(128 | (zero - 1));
                i += zero;
                continue;
            }
            u32 start = i;
            i++;
            while (i < n && i - start < 128)
                {
                    if (i + 2 < n && !p[i] && !p[i + 1] && !p[i + 2])
                        break;
                    i++;
                }
            u32 run = i - start;
            if (run + 1 > sizeof (packed) - k) {
                failed = 1;
                return 0;
            }
            packed[k++] = (u8)(run - 1);
            memcpy(packed + k, p + start, run);
            k += run;
        }
    return k;
}
static int unpack_checkpoint(u32 n) {
    if (n < 3 || packed[0] != 1 || ((u32)packed[1] | ((u32)packed[2] << 8)) != sizeof (cp))
        return 0;
    u32 i = 3, k = 0;
    u8 *p = (u8 *)&cp;
    while (i < n)
        {
            u32 c = packed[i++], run = (c & 127) + 1;
            if (run > sizeof (cp) - k)
                return 0;
            if (c & 128)
                memset(p + k, 0, run);
            else {
                if (run > n - i)
                    return 0;
                memcpy(p + k, packed + i, run);
                i += run;
            }
            k += run;
        }
    return k == sizeof (cp);
}
static u8 piece[512];
static u32 piece_n;
static int fetched;
static Span last_outcome;
static int cursor_valid(const Cursor *c, u32 limit) {
    return c->pos <= limit && c->start <= limit && c->n <= 128 && c->n <= limit - c->start;
}
/* Reject malformed persisted guest state before touching any cached span.
 * Trusted event action remains authoritative; checkpoint never supplies identity. */
static int checkpoint_valid(Span action, Span kind) {
    u32 expected = equal(kind, "\"input_changed\"") ? 1 : equal(kind, "\"move_paragraph\"") ? 2 : equal(kind, "\"replace_paragraph\"") ? 3 : 0;
    if (cp.magic != 4345905 || !expected || cp.action != expected || cp.phase < 1 || cp.phase > 5 || cp.total > 1048576 || cp.input_bytes > 1048576 || cp.from > 1048576 || cp.to > 1048576 || cp.count > 1048576 || cp.scan_index > 1048576 || cp.body_start > cp.total || cp.previous_cr > 1 || cp.previous_previous_cr > 1 || cp.selected_found > 1 || cp.selected_cr > 1 || cp.previous_body_cr > 1 || cp.last_empty > 1 || cp.penultimate_empty > 1 || cp.started > 1 || cp.ambiguous > 1 || cp.part > 1 || cp.tail_bytes > 2 || cp.target_separator > 2 || cp.previous_separator > 2 || cp.selected_start > cp.selected_end || cp.selected_end > cp.selected_next || cp.selected_next > cp.total || cp.normal_index > cp.count || cp.out_index > cp.count || cp.output_offset > 1048576)
        return 0;
    if (expected == 1 && cp.input_bytes != (u32)number(field(action, "total_bytes")))
        return 0;
    if (expected == 2 && (cp.from != (u32)number(field(action, "from")) || cp.to != (u32)number(field(action, "to"))))
        return 0;
    if (expected == 3 && (cp.from != (u32)number(field(action, "index")) || cp.input_bytes != (u32)number(field(action, "input_bytes"))))
        return 0;
    if (!cursor_valid(&cp.main, cp.total) || !cursor_valid(&cp.separator, cp.total) || !cursor_valid(&cp.moved, expected == 2 ? cp.total : cp.input_bytes))
        return 0;
    if (expected != 1 && cp.phase >= 2 && (!cp.selected_found || !cp.count || cp.from >= cp.count || (expected == 2 && cp.to >= cp.count)))
        return 0;
    if (cp.phase == 1 && cp.body_start > cp.main.pos)
        return 0;
    return !failed;
}
/* Public source/input reads; at most one data fetch per fresh VM. */
static void source_request(u32 offset) {
    request_n = 0;
    lit("{\"api_version\":{\"major\":0,\"minor\":3},\"draft_protocol\":1,\"request_id\":\"source\",\"resource\":");
    span(field(context, "resource"));
    lit(",\"mount_id\":");
    span(field(context, "mount_id"));
    lit(",\"operation\":{\"kind\":\"read_chunk\",\"revision\":");
    decimal(base);
    lit(",\"offset\":");
    decimal(offset);
    lit(",\"max_bytes\":128}}");
}
static void input_request(u32 offset) {
    request_n = 0;
    lit("{\"api_version\":{\"major\":0,\"minor\":3},\"context_protocol\":1,\"request_id\":\"input\",\"operation\":{\"kind\":\"read_input_chunk\",\"sequence\":");
    decimal(seq);
    lit(",\"expected_revision\":");
    decimal(base);
    lit(",\"offset\":");
    decimal(offset);
    lit(",\"max_bytes\":128}}");
}
static int available(Cursor *c, int input) {
    if (c->pos >= c->start && c->pos < c->start + c->n)
        return 1;
    if (fetched)
        return 0;
    fetched = 1;
    if (input)
        input_request(c->pos);
    else
        source_request(c->pos);
    Span r = rpc(reply);
    last_outcome = field(r, "outcome");
    u32 total = (u32)number(field(last_outcome, "total_bytes"));
    if (input) {
        if (total != cp.input_bytes)
            failed = 1;
    } else {
        if (cp.total && total != cp.total)
            failed = 1;
        cp.total = total;
    }
    c->start = c->pos;
    c->n = unquote(field(last_outcome, "text"), c->data, 128);
    if (!c->n && c->pos < total)
        failed = 1;
    return c->n != 0;
}
static u32 scalar_size(u8 b) {
    return b < 128 ? 1 : b < 224 ? 2 : b < 240 ? 3 : 4;
}
static int peek(Cursor *c, int input, u32 *ch, u32 *width) {
    if (!available(c, input))
        return 0;
    u32 at = c->pos - c->start;
    *ch = c->data[at];
    *width = scalar_size((u8)*ch);
    if (at + *width > c->n) {
        failed = 1;
        return 0;
    }
    return 1;
}
static int newline(Cursor *c, u32 *bytes) {
    u32 ch, w;
    if (!peek(c, 0, &ch, &w))
        return -1;
    if (ch == 10) {
        *bytes = 1;
        return 1;
    }
    if (ch != 13)
        return 0;
    if (c->pos + 1 >= cp.total)
        return 0;
    u32 at = c->pos - c->start;
    if (at + 1 >= c->n) {
        c->n = 0;
        return -1;
    }
    if (c->data[at + 1] == 10) {
        *bytes = 2;
        return 1;
    }
    return 0;
}
static int emit_scalar(Cursor *c, int input) {
    u32 ch, w;
    if (!peek(c, input, &ch, &w) || w > 512 - piece_n)
        return 0;
    memcpy(piece + piece_n, c->data + c->pos - c->start, w);
    piece_n += w;
    c->pos += w;
    return 1;
}
/* Streaming grammar scan and Move boundary-reparse safety checks. */
static void finish_body(u32 end, u32 next, u32 separator_bytes) {
    u32 index = cp.scan_index;
    u32 is_cr = cp.previous_cr, is_empty = end == cp.body_start;
    if (index == cp.from) {
        cp.selected_start = cp.body_start;
        cp.selected_end = end;
        cp.selected_next = next;
        cp.selected_found = 1;
        cp.selected_cr = is_cr;
    }
    if (index == cp.to)
        cp.target_separator = separator_bytes;
    if (cp.action == 2) {
        if (cp.from < cp.to && index > cp.from && index <= cp.to && is_cr && cp.previous_separator == 1)
            cp.ambiguous = 1;
        if (cp.from > cp.to && index > cp.to && index <= cp.from && cp.previous_body_cr && separator_bytes == 1)
            cp.ambiguous = 1;
    }
    cp.previous_body_cr = is_cr;
    cp.penultimate_empty = cp.last_empty;
    cp.last_empty = is_empty;
    cp.previous_separator = separator_bytes;
}
static void scan_step(void) {
    u32 steps = 0;
    while (!failed && steps++ < 64)
        {
            if (cp.main.pos == cp.total && cp.total) {
                finish_body(cp.total, cp.total, 0);
                cp.count = cp.scan_index + 1;
                cp.phase = 2;
                return;
            }
            u32 ch, w;
            if (!peek(&cp.main, 0, &ch, &w)) {
                if (!cp.total && !failed && fetched) {
                    finish_body(0, 0, 0);
                    cp.count = 1;
                    cp.phase = 2;
                }
                return;
            }
            if (ch == 10) {
                u32 end = cp.main.pos - (cp.previous_cr ? 1 : 0), sep = cp.previous_cr ? 2 : 1, next = cp.main.pos + 1;
                u32 body_cr = 0;
                if (end > cp.body_start) {
                    u32 a = end - 1;
                    if (a >= cp.main.start && a < cp.main.start + cp.main.n)
                        body_cr = cp.main.data[a - cp.main.start] == 13;
                    else
                        body_cr = cp.previous_previous_cr;
                }
                cp.previous_cr = body_cr;
                finish_body(end, next, sep);
                cp.main.pos = next;
                cp.previous_cr = 0;
                if (next == cp.total) {
                    cp.tail_bytes = sep;
                    cp.count = cp.scan_index + 1;
                    cp.phase = 2;
                    return;
                }
                cp.scan_index++;
                cp.body_start = next;
            } else {
                cp.previous_previous_cr = cp.previous_cr;
                cp.previous_cr = ch == 13;
                cp.main.pos += w;
            }
        }
}
static void start_output(void) {
    if (cp.from >= cp.count || (cp.action == 2 && cp.to >= cp.count)) {
        failed = 2;
        return;
    }
    if (cp.action == 2) {
        if (cp.from == cp.to) {
            cp.phase = 5;
            return;
        }
        if (cp.selected_cr && cp.target_separator == 1)
            cp.ambiguous = 1;
        if (!cp.tail_bytes && cp.count > 1) {
            u32 final = cp.count - 1;
            if (cp.to == final && cp.selected_start == cp.selected_end)
                cp.ambiguous = 1;
            if (cp.from == final && cp.to < final && cp.penultimate_empty)
                cp.ambiguous = 1;
        }
        if (cp.ambiguous) {
            failed = 3;
            return;
        }
    }
    if (cp.action == 3 && cp.input_bytes > 1048576 - (cp.total - (cp.selected_end - cp.selected_start))) {
        failed = 4;
        return;
    }
    cp.main.pos = 0;
    cp.main.n = 0;
    cp.moved.pos = cp.action == 3 ? 0 : cp.selected_start;
    cp.moved.n = 0;
    cp.separator.pos = 0;
    cp.separator.n = 0;
    cp.normal_index = 0;
    cp.out_index = 0;
    cp.part = 0;
    cp.phase = 3;
}
static u32 original_index(void) {
    u32 i = cp.out_index;
    if (cp.from < cp.to) {
        if (i >= cp.from && i < cp.to)
            return i + 1;
        if (i == cp.to)
            return cp.from;
    } else {
        if (i == cp.to)
            return cp.from;
        if (i > cp.to && i <= cp.from)
            return i - 1;
    }
    return i;
}
static int ordinal_separator(void) {
    u32 steps = 0;
    while (steps++ < 64)
        {
            if (cp.separator.pos >= cp.total - cp.tail_bytes) {
                failed = 1;
                return 0;
            }
            u32 sep;
            int n = newline(&cp.separator, &sep);
            if (n < 0)
                return 0;
            if (n) {
                if (piece_n + sep > 512)
                    return 0;
                memcpy(piece + piece_n, cp.separator.data + cp.separator.pos - cp.separator.start, sep);
                piece_n += sep;
                cp.separator.pos += sep;
                return 1;
            }
            u32 ch, w;
            if (!peek(&cp.separator, 0, &ch, &w))
                return 0;
            cp.separator.pos += w;
        }
    return 0;
}
/* Three independent cursors: normal body, moved/input body, ordinal separators. */
static void output_step(void) {
    u32 steps = 0;
    while (!failed && piece_n < 508 && steps++ < 64)
        {
            if (cp.out_index >= cp.count) {
                if (cp.tail_bytes) {
                    piece[piece_n++] = cp.tail_bytes == 2 ? 13 : 10;
                    if (cp.tail_bytes == 2)
                        piece[piece_n++] = 10;
                }
                cp.phase = 4;
                return;
            }
            if (cp.part == 1) {
                if (!ordinal_separator())
                    return;
                cp.out_index++;
                cp.part = 0;
                continue;
            }
            u32 orig = cp.action == 2 ? original_index() : cp.out_index;
            int selected = orig == cp.from;
            if (selected && (cp.action == 2 || cp.action == 3)) {
                u32 end = cp.action == 3 ? cp.input_bytes : cp.selected_end;
                if (cp.moved.pos < end) {
                    if (!emit_scalar(&cp.moved, cp.action == 3))
                        return;
                    continue;
                }
            } else {
                if (cp.normal_index == cp.from && (cp.action == 2 || cp.action == 3)) {
                    cp.main.pos = cp.selected_next;
                    cp.main.n = 0;
                    cp.normal_index++;
                }
                if (cp.normal_index != orig) {
                    failed = 1;
                    return;
                }
                if (cp.main.pos < cp.total - cp.tail_bytes) {
                    u32 sep;
                    int n = newline(&cp.main, &sep);
                    if (n < 0)
                        return;
                    if (!n) {
                        if (!emit_scalar(&cp.main, 0))
                            return;
                        continue;
                    }
                    cp.main.pos += sep;
                }
                cp.normal_index++;
            }
            if (cp.out_index + 1 < cp.count)
                cp.part = 1;
            else
                cp.out_index++;
        }
}
/* Text effect and checkpoint progress are one public atomic AdvanceEvent. */
static void advance(const char *kind, int complete) {
    identity();
    lit(",\"operation\":{\"kind\":\"advance_event\",\"expected_checkpoint_revision\":");
    decimal(checkpoint_revision);
    lit(",\"checkpoint_base64\":\"");
    b64encode(packed, pack_checkpoint());
    lit("\",\"complete\":");
    lit(complete ? "true" : "false");
    lit(",\"effect\":");
    if (!kind)
        lit("null");
    else if (equal((Span){kind, len(kind)}, "begin")) {
        lit("{\"kind\":\"begin_replace\",\"expected_revision\":");
        decimal(base);
        lit(",\"total_bytes\":");
        decimal(cp.action == 1 ? cp.input_bytes : cp.action == 2 ? cp.total : cp.total - (cp.selected_end - cp.selected_start) + cp.input_bytes);
        lit("}");
    } else if (equal((Span){kind, len(kind)}, "append")) {
        lit("{\"kind\":\"append_chunk\",\"offset\":");
        decimal(cp.output_offset - piece_n);
        lit(",\"text\":");
        quote_bytes(piece, piece_n);
        lit("}");
    } else
        lit("{\"kind\":\"commit_replace\"}");
    lit("}}");
    Span r = rpc(output);
    output_n = r.n;
}
/* Fresh-VM entry: Context + Metadata + checkpoint + one data RPC + Advance. */
u64 work(void) {
    request_n = 0;
    lit("{\"api_version\":{\"major\":0,\"minor\":3},\"context_protocol\":1,\"request_id\":\"context\",\"operation\":{\"kind\":\"read_context\"}}");
    context = rpc(ctx);
    seq = number(field(context, "sequence"));
    base = number(field(context, "expected_revision"));
    Span action = field(field(context, "outcome"), "action"), kind = field(action, "kind");
    identity();
    lit(",\"operation\":{\"kind\":\"read_metadata\"}}");
    Span m = rpc(meta);
    Span metadata = field(field(m, "outcome"), "metadata");
    checkpoint_revision = number(field(metadata, "checkpoint_revision"));
    if (!equal(field(metadata, "state"), "\"active\"")) {
        memcpy(output, meta, m.n);
        output_n = m.n;
        goto done;
    }
    u32 checkpoint_bytes = (u32)number(field(metadata, "checkpoint_bytes"));
    if (checkpoint_bytes) {
        identity();
        lit(",\"operation\":{\"kind\":\"read_checkpoint_chunk\",\"checkpoint_revision\":");
        decimal(checkpoint_revision);
        lit(",\"offset\":0,\"max_bytes\":2048}}");
        Span r = rpc(reply);
        u32 got = b64decode(field(field(r, "outcome"), "checkpoint_base64"), packed, sizeof (packed));
        if (!unpack_checkpoint(got) || !checkpoint_valid(action, kind))
            failed = 1;
    } else {
        memset(&cp, 0, sizeof (cp));
        cp.magic = 4345905;
        cp.phase = 1;
        if (equal(kind, "\"input_changed\"")) {
            cp.action = 1;
            cp.input_bytes = (u32)number(field(action, "total_bytes"));
            cp.phase = 3;
        } else if (equal(kind, "\"move_paragraph\"")) {
            cp.action = 2;
            cp.from = (u32)number(field(action, "from"));
            cp.to = (u32)number(field(action, "to"));
        } else if (equal(kind, "\"replace_paragraph\"")) {
            cp.action = 3;
            cp.from = (u32)number(field(action, "index"));
            cp.input_bytes = (u32)number(field(action, "input_bytes"));
        } else
            failed = 1;
    }
    if (failed)
        goto done;
    if (cp.phase == 1)
        scan_step();
    if (cp.phase == 2)
        start_output();
    if (failed)
        goto done;
    if (cp.phase == 5) {
        advance(0, 1);
        goto done;
    }
    if (cp.phase == 3 && !cp.started) {
        cp.started = 1;
        advance("begin", 0);
        goto done;
    }
    if (cp.phase == 3) {
        if (cp.action == 1) {
            while (piece_n < 508 && cp.moved.pos < cp.input_bytes)
                {
                    if (!emit_scalar(&cp.moved, 1))
                        break;
                }
            if (cp.moved.pos == cp.input_bytes)
                cp.phase = 4;
        } else
            output_step();
    }
    if (failed)
        goto done;
    if (piece_n) {
        cp.output_offset += piece_n;
        advance("append", 0);
    } else if (cp.phase == 4) {
        cp.phase = 5;
        advance("commit", 1);
    } else
        advance(0, 0);
  done:
    if (failed) {
        const char *msg = failed == 2 ? "{\"status\":\"rejected\",\"reason\":\"index\"}" : failed == 3 ? "{\"status\":\"rejected\",\"reason\":\"paragraph_boundary_conflict\"}" : failed == 4 ? "{\"status\":\"rejected\",\"reason\":\"text_budget\"}" : "{\"status\":\"rejected\",\"reason\":\"protocol\"}";
        output_n = len(msg);
        memcpy(output, msg, output_n);
    }
    return ((u64)output_n << 32) | (u32)output;
}
