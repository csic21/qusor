/* New opt-in B. Legacy edit algorithm remains byte-for-byte in its own file. */
#define work legacy_edit_work
#include "component_paragraph.c"
#undef work

typedef struct {
    u32 ordinal;
    u32 start;
    u32 end;
} ProjectionRow;
typedef struct {
    u32 magic;
    u32 version;
    u32 total;
    u32 first;
    u32 maximum;
    u32 position;
    u32 ordinal;
    u32 body_start;
    u32 previous_cr;
    u32 done;
    u32 eof;
    u32 count;
    u32 cache_start;
    u32 cache_n;
    ProjectionRow rows[8];
    u8 cache[128];
} ProjectionCheckpoint;
static ProjectionCheckpoint projection;
static u8 projection_raw[280];
static char projection_context_buffer[4096];
static Span projection_context;
static void projection_identity(void) {
    request_n = 0;
    lit("{\"kind\":\"query\",\"api_version\":{\"major\":0,\"minor\":3},\"projection_protocol\":1,\"request_id\":\"projection\",\"resource\":");
    span(field(projection_context, "resource"));
    lit(",\"mount_id\":");
    span(field(projection_context, "mount_id"));
    lit(",\"query_id\":");
    span(field(projection_context, "query_id"));
    lit(",\"base_revision\":");
    decimal(base);
}
static int projection_utf8(const u8 *s, u32 n, u32 cursor) {
    u32 boundary = cursor == 0;
    for (u32 i = 0; i < n;) {
        u8 a = s[i];
        u32 width = a < 128 ? 1 : (a >= 194 && a <= 223) ? 2 : (a >= 224 && a <= 239) ? 3 : (a >= 240 && a <= 244) ? 4 : 0;
        if (!width || width > n - i)
            return 0;
        for (u32 j = 1; j < width; j++)
            if ((s[i + j] & 192) != 128)
                return 0;
        if (width == 3 && ((a == 224 && s[i + 1] < 160) || (a == 237 && s[i + 1] >= 160)))
            return 0;
        if (width == 4 && ((a == 240 && s[i + 1] < 144) || (a == 244 && s[i + 1] >= 144)))
            return 0;
        i += width;
        if (i == cursor)
            boundary = 1;
    }
    return boundary;
}
static int projection_valid(u32 total, u32 first, u32 maximum) {
    ProjectionCheckpoint *p = &projection;
    if (p->magic != 1347570225 || p->version != 1 || p->total != total || p->first != first || p->maximum != maximum || !maximum || maximum > 8 || total > 1048576 || first > 1048576 || p->position > total || p->body_start > p->position || p->ordinal > 1048576 || p->previous_cr > 1 || p->done > 1 || p->eof > 1 || p->count > maximum || p->cache_n > 128 || p->cache_start > total || p->cache_n > total - p->cache_start || (p->eof && !p->done) || (p->done && !p->eof && p->count != maximum))
        return 0;
    if (p->cache_n && (p->position < p->cache_start || p->position > p->cache_start + p->cache_n || !projection_utf8(p->cache, p->cache_n, p->position - p->cache_start)))
        return 0;
    if (!p->cache_n && (p->position || p->cache_start))
        return 0;
    if (p->eof && p->position != total)
        return 0;
    if (p->ordinal > p->position || (p->ordinal >= first && !p->done && p->ordinal - first != p->count))
        return 0;
    if (p->previous_cr && (!p->cache_n || p->position <= p->cache_start || p->cache[p->position - p->cache_start - 1] != 13))
        return 0;
    if (p->previous_cr && (!p->position || p->position <= p->body_start))
        return 0;
    for (u32 i = 0; i < p->count; i++) {
        ProjectionRow *r = &p->rows[i];
        if (r->ordinal != first + i || r->ordinal >= (total ? total : 1) || r->ordinal > p->ordinal || r->start > r->end || r->end > total || r->end > p->position || (i && p->rows[i - 1].end > r->start))
            return 0;
    }
    if (p->count && p->rows[p->count - 1].ordinal >= p->ordinal && !p->done)
        return 0;
    if (p->count && p->eof && p->rows[p->count - 1].ordinal != p->ordinal)
        return 0;
    if (p->done && !p->eof && (p->ordinal != first + p->count || p->body_start != p->position))
        return 0;
    return 1;
}
static void projection_row(u32 end) {
    ProjectionCheckpoint *p = &projection;
    if (p->ordinal >= p->first && p->count < p->maximum) {
        ProjectionRow *r = &p->rows[p->count++];
        r->ordinal = p->ordinal;
        r->start = p->body_start;
        r->end = end;
    }
}
static void projection_step(void) {
    ProjectionCheckpoint *p = &projection;
    u32 did_fetch = 0;
    for (u32 steps = 0; steps < 64 && !p->done; steps++) {
        if (p->position == p->total) {
            projection_row(p->position);
            p->done = 1;
            p->eof = 1;
            break;
        }
        if (p->position < p->cache_start || p->position >= p->cache_start + p->cache_n) {
            if (did_fetch)
                break;
            did_fetch = 1;
            context = projection_context;
            source_request(p->position);
            Span r = rpc(reply), o = field(r, "outcome");
            if (number(field(o, "total_bytes")) != p->total || number(field(o, "revision")) != base || number(field(o, "offset")) != p->position) {
                failed = 1;
                return;
            }
            p->cache_start = p->position;
            p->cache_n = unquote(field(o, "text"), p->cache, 128);
            if (!p->cache_n || p->cache_n > p->total - p->position) {
                failed = 1;
                return;
            }
        }
        u32 at = p->position - p->cache_start;
        u8 b = p->cache[at];
        u32 width = scalar_size(b);
        if (!width || width > p->cache_n - at || !projection_utf8(p->cache + at, width, width)) {
            failed = 1;
            return;
        }
        if (b == 10) {
            projection_row(p->position - p->previous_cr);
            p->position++;
            p->previous_cr = 0;
            if (p->position == p->total) {
                p->done = 1;
                p->eof = 1;
            } else {
                p->ordinal++;
                p->body_start = p->position;
                if (p->count == p->maximum)
                    p->done = 1;
            }
        } else {
            p->position += width;
            p->previous_cr = (width == 1 && b == 13);
        }
    }
    if (!p->done && p->position == p->total) {
        projection_row(p->position);
        p->done = 1;
        p->eof = 1;
    }
}
static void projection_window(void) {
    ProjectionCheckpoint *p = &projection;
    u32 partial = !p->done && p->ordinal >= p->first && p->count < p->maximum;
    if (!p->count && !partial && !p->eof) {
        lit("null");
        return;
    }
    lit("{\"rows\":[");
    for (u32 i = 0; i < p->count + partial; i++) {
        if (i)
            lit(",");
        u32 ordinal = i < p->count ? p->rows[i].ordinal : p->ordinal;
        u32 start = i < p->count ? p->rows[i].start : p->body_start;
        u32 end = i < p->count ? p->rows[i].end : p->position - p->previous_cr;
        lit("{\"ordinal\":");
        decimal(ordinal);
        lit(",\"start\":");
        decimal(start);
        lit(",\"end\":");
        decimal(end);
        lit(",\"complete\":");
        lit(i < p->count ? "true" : "false");
        lit("}");
    }
    lit("],\"reached_eof\":");
    lit(p->eof ? "true" : "false");
    lit("}");
}
static u64 projection_work(void) {
    request_n = 0;
    lit("{\"kind\":\"read_context\",\"api_version\":{\"major\":0,\"minor\":3},\"projection_protocol\":1,\"request_id\":\"projection-context\"}");
    Span r = rpc(projection_context_buffer);
    projection_context = field(r, "context");
    u64 total64 = number(field(projection_context, "total_bytes")), first64 = number(field(projection_context, "first_paragraph")), max64 = number(field(projection_context, "max_rows"));
    base = number(field(projection_context, "base_revision"));
    if (!base || total64 > 1048576 || first64 > 1048576 || !max64 || max64 > 8) {
        failed = 1;
        goto end;
    }
    projection_identity();
    lit(",\"operation\":{\"kind\":\"read_metadata\"}}");
    r = rpc(meta);
    Span m = field(field(r, "outcome"), "metadata");
    checkpoint_revision = number(field(m, "checkpoint_revision"));
    if (!checkpoint_revision) {
        failed = 1;
        goto end;
    }
    if (!equal(field(m, "state"), "\"active\"")) {
        memcpy(output, meta, r.n);
        output_n = r.n;
        goto end;
    }
    u64 size = number(field(m, "checkpoint_bytes"));
    if (size) {
        if (size != sizeof (projection)) {
            failed = 1;
            goto end;
        }
        projection_identity();
        lit(",\"operation\":{\"kind\":\"read_checkpoint_chunk\",\"checkpoint_revision\":");
        decimal(checkpoint_revision);
        lit(",\"offset\":0,\"max_bytes\":2048}}");
        r = rpc(reply);
        Span o = field(r, "outcome");
        if (number(field(o, "checkpoint_revision")) != checkpoint_revision || number(field(o, "offset")) != 0 || number(field(o, "total_bytes")) != sizeof (projection)) {
            failed = 1;
            goto end;
        }
        u32 got = b64decode(field(o, "checkpoint_base64"), projection_raw, sizeof (projection_raw));
        if (got != sizeof (projection)) {
            failed = 1;
            goto end;
        }
        memcpy(&projection, projection_raw, sizeof (projection));
        if (!projection_valid((u32)total64, (u32)first64, (u32)max64)) {
            failed = 1;
            goto end;
        }
    } else {
        memset(&projection, 0, sizeof (projection));
        projection.magic = 1347570225;
        projection.version = 1;
        projection.total = (u32)total64;
        projection.first = (u32)first64;
        projection.maximum = (u32)max64;
    }
    if (failed)
        goto end;
    projection_step();
    if (failed)
        goto end;
    projection_identity();
    lit(",\"operation\":{\"kind\":\"advance_projection\",\"expected_checkpoint_revision\":");
    decimal(checkpoint_revision);
    lit(",\"checkpoint_base64\":\"");
    b64encode((const u8 *)&projection, sizeof (projection));
    lit("\",\"window\":");
    projection_window();
    lit(",\"complete\":");
    lit(projection.done ? "true" : "false");
    lit("}}");
    r = rpc(output);
    output_n = r.n;
  end:
    if (failed) {
        const char *s = "{\"status\":\"rejected\",\"reason\":\"projection_protocol\"}";
        output_n = len(s);
        memcpy(output, s, output_n);
    }
    return ((u64)output_n << 32) | (u32)output;
}
u64 work(void) {
    request_n = 0;
    lit("{\"api_version\":{\"major\":0,\"minor\":3},\"component_invocation_protocol\":1,\"request_id\":\"bootstrap\",\"operation\":{\"kind\":\"read_context\"}}");
    Span r = rpc(reply), purpose = field(field(r, "context"), "kind");
    if (!failed && equal(purpose, "\"edit_event\""))
        return legacy_edit_work();
    if (!failed && equal(purpose, "\"paragraph_projection\""))
        return projection_work();
    const char *s = "{\"status\":\"rejected\",\"reason\":\"bootstrap_protocol\"}";
    output_n = len(s);
    memcpy(output, s, output_n);
    return ((u64)output_n << 32) | (u32)output;
}

