export type PageGlyph = [
    scalar: number,
    x: number,
    y: number,
    width: number,
    height: number,
];

export type PageGeometry = {
    width: number;
    height: number;
    glyphs: PageGlyph[];
};

export type PageSelection = {
    text: string;
    start: number;
    boxes: [number, number, number, number][];
};

export type SavedSelectionRange = {
    capture_id: number;
    start: number;
    end: number;
};

const MAX_GEOMETRY_GLYPHS = 20_000;

function finite(value: unknown): value is number {
    return typeof value === "number" && Number.isFinite(value);
}

export function parsePageGeometry(
    raw: string,
    expectedWidth: number,
    expectedHeight: number,
): PageGeometry | null {
    if (!raw || raw.length > 512 * 1024) return null;
    try {
        const value = JSON.parse(raw);
        if (
            !finite(value?.width) ||
            !finite(value?.height) ||
            value.width !== expectedWidth ||
            value.height !== expectedHeight ||
            !Array.isArray(value?.glyphs) ||
            value.glyphs.length > MAX_GEOMETRY_GLYPHS
        ) {
            return null;
        }
        const glyphs: PageGlyph[] = [];
        let previous = -1;
        for (const rawGlyph of value.glyphs) {
            if (
                !Array.isArray(rawGlyph) ||
                rawGlyph.length !== 5 ||
                !rawGlyph.every(finite)
            ) {
                return null;
            }
            const [scalar, x, y, width, height] = rawGlyph;
            if (
                !Number.isSafeInteger(scalar) ||
                scalar < previous ||
                scalar < 0 ||
                width <= 0 ||
                height <= 0
            ) {
                return null;
            }
            previous = scalar;
            glyphs.push([scalar, x, y, width, height]);
        }
        return {
            width: value.width,
            height: value.height,
            glyphs,
        };
    } catch (_) {
        return null;
    }
}

export function glyphAt(
    geometry: PageGeometry,
    display: { width: number; height: number },
    x: number,
    y: number,
): number | null {
    if (display.width <= 0 || display.height <= 0) return null;
    const pageX = (x / display.width) * geometry.width;
    const pageY = (y / display.height) * geometry.height;
    let found: number | null = null;
    let smallest = Number.POSITIVE_INFINITY;
    for (let index = 0; index < geometry.glyphs.length; index += 1) {
        const [, left, top, width, height] = geometry.glyphs[index];
        if (
            pageX >= left &&
            pageX <= left + width &&
            pageY >= top &&
            pageY <= top + height
        ) {
            const area = width * height;
            if (area < smallest) {
                smallest = area;
                found = index;
            }
        }
    }
    return found;
}

export function selectionBetween(
    geometry: PageGeometry,
    pageText: string,
    first: number,
    last: number,
): PageSelection | null {
    if (
        first < 0 ||
        last < 0 ||
        first >= geometry.glyphs.length ||
        last >= geometry.glyphs.length
    ) {
        return null;
    }
    const low = Math.min(first, last);
    const high = Math.max(first, last);
    const start = geometry.glyphs[low][0];
    const end = geometry.glyphs[high][0] + 1;
    const scalars = Array.from(pageText);
    if (start >= scalars.length || end > scalars.length) return null;
    const text = scalars.slice(start, end).join("");
    if (!text.trim()) return null;
    return {
        text,
        start,
        boxes: geometry.glyphs
            .slice(low, high + 1)
            .map(([, x, y, width, height]) => [x, y, width, height]),
    };
}

export function boxesForScalarRange(
    geometry: PageGeometry,
    start: number,
    end: number,
): [number, number, number, number][] {
    if (
        !Number.isSafeInteger(start) ||
        !Number.isSafeInteger(end) ||
        start < 0 ||
        start >= end
    ) {
        return [];
    }
    return geometry.glyphs
        .filter(([scalar]) => scalar >= start && scalar < end)
        .map(([, x, y, width, height]) => [x, y, width, height]);
}

export function boxesForSavedRanges(
    geometry: PageGeometry,
    ranges: readonly SavedSelectionRange[],
): [number, number, number, number][] {
    const merged: [number, number][] = [];
    for (const { start, end } of ranges
        .filter(
            ({ start, end }) =>
                Number.isSafeInteger(start) &&
                Number.isSafeInteger(end) &&
                start >= 0 &&
                start < end,
        )
        .sort((left, right) => left.start - right.start || left.end - right.end)) {
        const previous = merged[merged.length - 1];
        if (previous && start <= previous[1]) {
            previous[1] = Math.max(previous[1], end);
        } else {
            merged.push([start, end]);
        }
    }
    if (!merged.length) return [];

    const boxes: [number, number, number, number][] = [];
    let rangeIndex = 0;
    for (const [scalar, x, y, width, height] of geometry.glyphs) {
        while (
            rangeIndex < merged.length &&
            scalar >= merged[rangeIndex][1]
        ) {
            rangeIndex += 1;
        }
        if (rangeIndex >= merged.length) break;
        if (
            scalar >= merged[rangeIndex][0] &&
            scalar < merged[rangeIndex][1]
        ) {
            boxes.push([x, y, width, height]);
        }
    }
    return boxes;
}
