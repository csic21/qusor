/// Pronunciation resources of the sample wordbooks, not part of the Core study
/// data: the public 0.5 wire carries the term, meaning and example only. A
/// production wordbook import will bring its own phonetics; until then these
/// entries cover the synthetic books the workbench seeds, and the system voice
/// still reads any term the Core returns.
export type VocabPronunciation = {
    /// Part of speech shown with the phonetic, e.g. `v.` or `n./v.`.
    pos: string;
    /// IPA of the American reading.
    us: string;
    /// IPA of the British reading; the same as `us` where the accents agree.
    uk: string;
};

export const vocabPronunciations: Record<string, VocabPronunciation> = {
    abandon: { pos: "v.", us: "/əˈbændən/", uk: "/əˈbændən/" },
    benefit: { pos: "n./v.", us: "/ˈbenɪfɪt/", uk: "/ˈbenɪfɪt/" },
    candidate: { pos: "n.", us: "/ˈkændɪdət/", uk: "/ˈkændɪdeɪt/" },
    coherent: { pos: "adj.", us: "/koʊˈhɪrənt/", uk: "/kəʊˈhɪərənt/" },
    cite: { pos: "v.", us: "/saɪt/", uk: "/saɪt/" },
    plugin: { pos: "n.", us: "/ˈplʌɡɪn/", uk: "/ˈplʌɡɪn/" },
    vault: { pos: "n.", us: "/vɔːlt/", uk: "/vɔːlt/" },
};

/// One card's pronunciation entry; `null` when the sample books do not carry
/// the term, so the card says so instead of inventing a phonetic.
export function pronunciationOf(term: string): VocabPronunciation | null {
    return vocabPronunciations[term.trim().toLowerCase()] ?? null;
}

/// The phonetic for the selected accent, or an empty string when the term has
/// no sample entry.
export function phoneticOf(term: string, accent: string): string {
    const entry = pronunciationOf(term);
    if (!entry) return "";
    return accent === "uk" ? entry.uk : entry.us;
}
