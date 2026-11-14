import { effect, keymap, Picker, Scroll, signal, Storage, View } from "nsh";
import { Button } from "nsh/base";
import { BackHandler } from "nsh/mobile";
import { VocabHost } from "qusor/pdf-surface";
import { Badge, Copy, focus, Glyph, ink, Rule, TextButton, Tool } from "../../ui";
import { phoneticOf, pronunciationOf } from "../../pronunciation";

// ---------------------------------------------------------------------------
// 背单词工作区：词书、学习卡与四档评分。
// The fixture lives in this window; when the host exposes the vocabulary module
// the workspace reads the local Core first and falls back here, saying so.
// ---------------------------------------------------------------------------
type VocabCard = {
    cardId: number;
    term: string;
    meaning: string;
    example: string;
    stage: string;
    pos?: string;
    phonetic?: string;
};
type VocabWordRow = {
    id: number;
    term: string;
    meaning: string;
    example: string;
    pos?: string;
    phonetic?: string;
};
type VocabDeck = {
    id: number;
    title: string;
    total: number;
    due: number;
    new: number;
};
type VocabOperation = {
    cardId: number;
    rating: string;
    id: string;
    attempts: number;
};
const vocabDecksFixture: VocabDeck[] = [
    { id: 1, title: "CET-4 示例", total: 3, due: 0, new: 3 },
    { id: 2, title: "学术写作 示例", total: 2, due: 0, new: 2 },
];
const vocabCardsFixture: Record<number, VocabCard[]> = {
    1: [
        {
            cardId: 1,
            term: "abandon",
            meaning: "放弃；抛弃",
            example: "They had to abandon the plan when the funding was withdrawn.",
            stage: "new",
        },
        {
            cardId: 2,
            term: "benefit",
            meaning: "好处；使受益",
            example: "Daily review benefits long-term memory more than one long session.",
            stage: "new",
        },
        {
            cardId: 3,
            term: "candidate",
            meaning: "候选人；应试者",
            example: "Each candidate read the passage twice before answering.",
            stage: "new",
        },
    ],
    2: [
        {
            cardId: 4,
            term: "coherent",
            meaning: "连贯的；一致的",
            example: "A coherent argument states its claim before the evidence.",
            stage: "new",
        },
        {
            cardId: 5,
            term: "cite",
            meaning: "引用；举例",
            example: "Cite the original page instead of a summary of it.",
            stage: "new",
        },
    ],
};
/// The four grades the vocabulary wire accepts, with the FSRS-5 interval the
/// spec expects for a new card.
const vocabRatings = [
    { rating: "again", label: "不认识", hint: "约 10 分钟" },
    { rating: "hard", label: "模糊", hint: "约 30 分钟" },
    { rating: "good", label: "认识", hint: "约 3 天" },
    { rating: "easy", label: "很熟", hint: "约 16 天" },
];
const vocabStageLabels: Record<string, string> = {
    new: "新词",
    learning: "学习中",
    review: "复习",
};
const vocabDecks = signal<VocabDeck[]>([]);
const vocabDeck = signal(0);
const vocabQueue = signal<VocabCard[]>([]);
const vocabRemainingDue = signal(0);
const vocabRemainingNew = signal(0);
const vocabLoading = signal(true);
const vocabReady = signal(false);
const vocabBusy = signal(false);
const vocabError = signal("");
const vocabNotice = signal("");
const vocabLive = signal(false);
const vocabSource = signal("样例数据");
const vocabSourceDetail = signal("窗口内样例");
const vocabOperation = signal<VocabOperation | null>(null);
const vocabRevealed = signal(false);
/// The accent the sample pronunciation service reads and shows: a window
/// preference, not Core state.
const vocabAccent = signal("us");
/// The text the system voice is reading right now (drives the speaker state).
const vocabSpeaking = signal("");
const vocabMobileView = signal<"decks" | "study" | "words">("decks");
const vocabDailyNewLimit = signal(10);
const vocabAutoSpeak = signal(false);
const vocabSlowSpeak = signal(false);
const vocabWordsDeckId = signal(0);
const vocabWordsList = signal<VocabWordRow[]>([]);
const vocabWordsLoading = signal(false);
const VOCAB_DAILY_LIMIT_KEY = "qusor.vocab.daily-new-limit.v1";
const VOCAB_PRONOUNCE_PREFS_KEY = "qusor.vocab.pronounce-prefs.v1";
const VOCAB_DAILY_LIMIT_OPTIONS = [5, 10, 20, 30] as const;
const MAX_WORDBOOK_IMPORT_BYTES = 2 * 1024 * 1024;

async function restoreVocabPreferences() {
    try {
        const rawLimit = await Storage.getItem(VOCAB_DAILY_LIMIT_KEY);
        if (rawLimit) {
            const parsed = Number(JSON.parse(rawLimit)?.limit);
            if (
                VOCAB_DAILY_LIMIT_OPTIONS.includes(
                    parsed as (typeof VOCAB_DAILY_LIMIT_OPTIONS)[number],
                )
            ) {
                vocabDailyNewLimit.set(parsed);
            }
        }
    } catch (_) {}
    try {
        const rawPrefs = await Storage.getItem(VOCAB_PRONOUNCE_PREFS_KEY);
        if (rawPrefs) {
            const parsed = JSON.parse(rawPrefs);
            if (typeof parsed?.autoSpeak === "boolean") {
                vocabAutoSpeak.set(parsed.autoSpeak);
            }
            if (typeof parsed?.slowSpeak === "boolean") {
                vocabSlowSpeak.set(parsed.slowSpeak);
            }
        }
    } catch (_) {}
}

async function changeVocabDailyNewLimit(limit: number) {
    vocabDailyNewLimit.set(limit);
    try {
        await Storage.setItem(
            VOCAB_DAILY_LIMIT_KEY,
            JSON.stringify({ limit }),
        );
    } catch (_) {}
    if (vocabDeck() > 0 && vocabLive()) {
        void loadVocabQueue(vocabDeck());
    }
}

async function toggleVocabAutoSpeak() {
    const next = !vocabAutoSpeak();
    vocabAutoSpeak.set(next);
    try {
        await Storage.setItem(
            VOCAB_PRONOUNCE_PREFS_KEY,
            JSON.stringify({
                autoSpeak: next,
                slowSpeak: vocabSlowSpeak(),
            }),
        );
    } catch (_) {}
}

async function toggleVocabSlowSpeak() {
    const next = !vocabSlowSpeak();
    vocabSlowSpeak.set(next);
    try {
        await Storage.setItem(
            VOCAB_PRONOUNCE_PREFS_KEY,
            JSON.stringify({
                autoSpeak: vocabAutoSpeak(),
                slowSpeak: next,
            }),
        );
    } catch (_) {}
}

function vocabDeckOf(id: number) {
    return vocabDecks().find((deck) => deck.id === id);
}
function currentVocabCard() {
    return vocabQueue()[0];
}
function vocabRatingLabel(rating: string) {
    const entry = vocabRatings.find((item) => item.rating === rating);
    return entry ? entry.label : rating;
}
function vocabStageLabel(stage: string) {
    return vocabStageLabels[stage] ?? "复习";
}
function vocabAccentLabel(accent: string) {
    return accent === "uk" ? "英音" : "美音";
}
/// Switching the reading never touches Core: it changes the IPA shown and the
/// voice the speaker asks for.
function chooseVocabAccent(accent: string) {
    if (vocabAccent() === accent) return;
    vocabAccent.set(accent);
    vocabSpeaking.set("");
    vocabNotice.set("");
}
function vocabNoticeForMissingVoice() {
    vocabNotice.set("当前宿主没有系统读音；先照音标读一遍再揭晓。");
}
/// The sample host asks a system voice to read a term or an example. A window
/// without that host says so instead of pretending it played something.
async function speakVocabText(text: string, label: string) {
    const spoken = text.trim();
    if (!spoken) return;
    vocabError.set("");
    const host = VocabHost as {
        pronounce?: (text: string, accent: string) => string;
        pronounceWithRate?: (
            text: string,
            accent: string,
            rate: number,
        ) => string;
    };
    const hasPronounceWithRate = typeof host.pronounceWithRate === "function";
    const hasPronounce = typeof host.pronounce === "function";
    if (!hasPronounceWithRate && !hasPronounce) {
        vocabNoticeForMissingVoice();
        return;
    }
    try {
        const accent = vocabAccent();
        const rate = vocabSlowSpeak() ? 0.68 : 1.0;
        const raw = hasPronounceWithRate
            ? await host.pronounceWithRate!(spoken, accent, rate)
            : await host.pronounce!(spoken, accent);
        const value = vocabAnswer(raw) as {
            voice?: string | null;
        };
        const voice = value.voice ? `（${value.voice}）` : "";
        vocabSpeaking.set(spoken);
        vocabNotice.set(`正在朗读${label} · ${vocabAccentLabel(accent)}${voice}`);
        setTimeout(() => {
            if (vocabSpeaking() === spoken) vocabSpeaking.set("");
        }, 1600);
    } catch (error) {
        if (/runtime unavailable/i.test(vocabFailureText(error))) {
            vocabNoticeForMissingVoice();
            return;
        }
        vocabNotice.set(`朗读没有开始：${vocabFailureText(error)}`);
    }
}
function speakVocabWord() {
    const card = currentVocabCard();
    if (card) void speakVocabText(card.term, `「${card.term}」`);
}
function speakVocabExample() {
    const card = currentVocabCard();
    if (card && vocabRevealed()) void speakVocabText(card.example, "例句");
}
export function vocabFailureText(error: unknown) {
    if (error && typeof error === "object" && "message" in error)
        return String((error as { message: unknown }).message);
    return String(error ?? "未知错误");
}
/// One user action keeps one operation id: a retry of the same click must not
/// schedule the card twice, so the id is only minted when the click is new.
function vocabOperationId(card: VocabCard, rating: string) {
    const nonce = Math.random().toString(36).slice(2, 10);
    return `vocab-rate-${card.cardId}-${rating}-${Date.now().toString(36)}-${nonce}`;
}
export function vocabAnswer(raw: string) {
    const result = JSON.parse(raw);
    if (!result || result.ok !== true)
        throw new Error(String(result?.error ?? "本机 Core 未返回结果"));
    return result.value ?? {};
}
function mapVocabCard(wire: any): VocabCard {
    return {
        cardId: Number(wire.card_id),
        term: String(wire.term ?? ""),
        meaning: String(wire.meaning ?? ""),
        example: String(wire.example ?? ""),
        stage: String(wire.stage ?? "new"),
        pos:
            typeof wire.pos === "string" && wire.pos.trim()
                ? wire.pos.trim()
                : undefined,
        phonetic:
            typeof wire.phonetic === "string" && wire.phonetic.trim()
                ? wire.phonetic.trim()
                : undefined,
    };
}
function mapVocabDeck(wire: any): VocabDeck {
    return {
        id: Number(wire.wordbook_id),
        title: String(wire.title ?? "词书"),
        total: Number(wire.total ?? 0),
        due: Number(wire.due ?? 0),
        new: Number(wire.new ?? 0),
    };
}
/// The sidebar shows the book's own due/new counters, so a committed review
/// asks the Core for them again instead of trusting the window's copy.
async function refreshVocabDecks() {
    if (!vocabLive()) return;
    try {
        const value = vocabAnswer(await VocabHost.decks());
        const decks: VocabDeck[] = Array.isArray(value.decks)
            ? value.decks.map(mapVocabDeck)
            : [];
        if (decks.length > 0) vocabDecks.set(decks);
    } catch (_) {
        // The review already went through; keep the last readable counters.
    }
}
/// The fixture has no Core behind it, so one graded card lowers the same
/// counter the deck row shows. Card stage decides which of due/new moves.
function decrementVocabDeck(card: VocabCard) {
    const id = vocabDeck();
    vocabDecks.set(
        vocabDecks().map((deck) => {
            if (deck.id !== id) return deck;
            if (card.stage === "new")
                return { ...deck, new: Math.max(0, deck.new - 1) };
            return { ...deck, due: Math.max(0, deck.due - 1) };
        }),
    );
}
function revealVocabAnswer() {
    if (currentVocabCard()) setVocabRevealed(true);
}
/// Hiding the answer drops the speaker state too, so the next card never shows
/// the previous term as "reading".
function setVocabRevealed(revealed: boolean) {
    vocabRevealed.set(revealed);
    if (!revealed) vocabSpeaking.set("");
}
function loadVocabFixtureQueue(id: number) {
    // Picking a sample wordbook starts its sample queue again, so the deck
    // row and the remaining counters both come from the fixture cards.
    const deck = vocabDecksFixture.find((item) => item.id === id);
    const cards = vocabCardsFixture[id] ?? [];
    vocabQueue.set(cards.map((card) => ({ ...card })));
    setVocabRevealed(false);
    vocabRemainingDue.set(cards.filter((card) => card.stage !== "new").length);
    vocabRemainingNew.set(cards.filter((card) => card.stage === "new").length);
    if (deck)
        vocabDecks.set(
            vocabDecks().map((item) => (item.id === id ? { ...deck } : item)),
        );
    vocabLoading.set(false);
    vocabBusy.set(false);
    vocabReady.set(true);
}
/// Any failed Core read falls back to the window fixture. The header then says
/// 样例数据 instead of 本机 Core and the reason stays visible.
function useVocabFixture(reason: string) {
    const previous = vocabDeck();
    const sameDeck = vocabDecksFixture.find((deck) => deck.id === previous);
    vocabLive.set(false);
    vocabSource.set("样例数据");
    vocabSourceDetail.set("窗口内样例");
    vocabDecks.set(vocabDecksFixture);
    vocabDeck.set(sameDeck ? sameDeck.id : 0);
    vocabQueue.set([]);
    setVocabRevealed(false);
    vocabRemainingDue.set(0);
    vocabRemainingNew.set(0);
    vocabOperation.set(null);
    vocabError.set(reason);
    vocabNotice.set("");
    vocabLoading.set(false);
    vocabBusy.set(false);
    vocabReady.set(true);
    if (sameDeck) loadVocabFixtureQueue(sameDeck.id);
}
export async function loadVocabDecks(retrying = false) {
    vocabLoading.set(true);
    try {
        const value = vocabAnswer(await VocabHost.decks());
        const decks: VocabDeck[] = Array.isArray(value.decks)
            ? value.decks.map(mapVocabDeck)
            : [];
        if (decks.length === 0) throw new Error("词书列表为空");
        vocabDecks.set(decks);
        vocabLive.set(true);
        vocabSource.set("本机 Core");
        vocabSourceDetail.set("由 vocab 插件读取");
        vocabDeck.set(0);
        vocabQueue.set([]);
        setVocabRevealed(false);
        vocabRemainingDue.set(0);
        vocabRemainingNew.set(0);
        vocabError.set("");
    } catch (_) {
        // Headless tests and a plain development host have no native module:
        // the fixture stays and the header names it as a sample.
        vocabDecks.set(vocabDecksFixture);
        vocabLive.set(false);
        vocabSource.set("样例数据");
        vocabSourceDetail.set("窗口内样例");
        vocabDeck.set(0);
        vocabQueue.set([]);
        setVocabRevealed(false);
        vocabRemainingDue.set(0);
        vocabRemainingNew.set(0);
        vocabError.set(
            retrying ? "仍读不到本机 Core，继续使用样例数据。" : "",
        );
    }
    vocabLoading.set(false);
    vocabReady.set(true);
}
function retryVocabDecks() {
    vocabError.set("");
    vocabLoading.set(true);
    void loadVocabDecks(true);
}

async function importVocabWordbook() {
    if (vocabBusy()) return;
    vocabError.set("");
    vocabNotice.set("");
    vocabBusy.set(true);
    try {
        const picked = await Picker.pickFiles({
            multiple: false,
            filters: [{ name: "QUSOR 词书", extensions: ["json"] }],
        });
        if (picked.length === 0) {
            vocabBusy.set(false);
            return;
        }
        const file = picked[0].file;
        if (file.size > MAX_WORDBOOK_IMPORT_BYTES) {
            throw new Error("词书 JSON 不能超过 2 MiB");
        }
        const wordbookJson = await file.text();
        if (!wordbookJson.trim()) throw new Error("词书 JSON 是空的");
        const value = vocabAnswer(
            await VocabHost.importWordbook(wordbookJson),
        );
        await loadVocabDecks();
        if (!vocabLive()) {
            vocabBusy.set(false);
            vocabNotice.set("词书已写入 Core，但列表读取失败；可重试读取。");
            return;
        }
        const wordbookId = Number(value.wordbook_id);
        vocabBusy.set(false);
        if (Number.isSafeInteger(wordbookId) && wordbookId > 0) {
            selectVocabDeck(wordbookId);
        }
        vocabNotice.set(
            `已导入「${String(value.title ?? "词书")}」 · ${Number(value.total ?? 0)} 张卡`,
        );
    } catch (error) {
        vocabBusy.set(false);
        vocabError.set(`导入词书失败：${vocabFailureText(error)}`);
    }
}

async function loadVocabQueue(id: number) {
    try {
        const host = VocabHost as any;
        const limit = vocabDailyNewLimit();
        const raw =
            typeof host?.queueWithLimit === "function"
                ? await host.queueWithLimit(id, limit)
                : await VocabHost.queue(id, limit);
        const value = vocabAnswer(raw);
        const cards: VocabCard[] = Array.isArray(value.cards)
            ? value.cards.map(mapVocabCard)
            : [];
        vocabQueue.set(cards);
        setVocabRevealed(false);
        vocabRemainingDue.set(Number(value.remaining_due ?? 0));
        vocabRemainingNew.set(Number(value.remaining_new ?? 0));
        vocabError.set("");
        if (vocabAutoSpeak() && cards[0]) {
            void speakVocabText(cards[0].term, `「${cards[0].term}」`);
        }
    } catch (error) {
        useVocabFixture(
            `复习队列读取失败（${vocabFailureText(error)}），已回退样例数据。`,
        );
    }
}
function selectVocabDeck(id: number) {
    vocabDeck.set(id);
    vocabMobileView.set("study");
    vocabQueue.set([]);
    setVocabRevealed(false);
    vocabNotice.set("");
    vocabError.set("");
    vocabOperation.set(null);
    vocabLoading.set(true);
    vocabBusy.set(true);
    if (vocabLive()) {
        void (async () => {
            await loadVocabQueue(id);
            vocabLoading.set(false);
            vocabBusy.set(false);
        })();
        return;
    }
    loadVocabFixtureQueue(id);
    const first = vocabQueue()[0];
    if (vocabAutoSpeak() && first) {
        void speakVocabText(first.term, `「${first.term}」`);
    }
}

async function openVocabWordbookCards(deckId: number) {
    vocabWordsDeckId.set(deckId);
    vocabMobileView.set("words");
    vocabWordsLoading.set(true);
    vocabError.set("");
    try {
        const host = VocabHost as any;
        if (vocabLive() && typeof host?.wordbookCards === "function") {
            const value = vocabAnswer(await host.wordbookCards(deckId, 200, 0));
            const rows: VocabWordRow[] = Array.isArray(value.cards)
                ? value.cards.map((raw: any) => ({
                      id: Number(raw.id ?? raw.card_id ?? 0),
                      term: String(raw.term ?? ""),
                      meaning: String(raw.meaning ?? ""),
                      example: String(raw.example ?? ""),
                      pos:
                          typeof raw.pos === "string" && raw.pos.trim()
                              ? raw.pos.trim()
                              : undefined,
                      phonetic:
                          typeof raw.phonetic === "string" &&
                          raw.phonetic.trim()
                              ? raw.phonetic.trim()
                              : undefined,
                  }))
                : [];
            vocabWordsList.set(rows);
            return;
        }
        const fallback = (vocabCardsFixture[deckId] ?? []).map((card) => ({
            id: card.cardId,
            term: card.term,
            meaning: card.meaning,
            example: card.example,
            pos: card.pos,
            phonetic: card.phonetic,
        }));
        vocabWordsList.set(fallback);
    } catch (error) {
        vocabError.set(`读取词表未完成：${vocabFailureText(error)}`);
        vocabWordsList.set([]);
    } finally {
        vocabWordsLoading.set(false);
    }
}

async function deleteVocabWordbook(deckId: number) {
    if (vocabBusy()) return;
    const targetDeck = vocabDeckOf(deckId);
    vocabBusy.set(true);
    vocabError.set("");
    vocabNotice.set("");
    try {
        const host = VocabHost as any;
        if (vocabLive() && typeof host?.deleteWordbook === "function") {
            vocabAnswer(await host.deleteWordbook(deckId));
            await loadVocabDecks();
        } else {
            vocabDecks.set(
                vocabDecks().filter((item) => item.id !== deckId),
            );
        }
        if (vocabDeck() === deckId) {
            vocabDeck.set(0);
            vocabQueue.set([]);
        }
        if (vocabWordsDeckId() === deckId) {
            vocabWordsDeckId.set(0);
            vocabWordsList.set([]);
        }
        vocabMobileView.set("decks");
        vocabNotice.set(`已删除词书「${targetDeck?.title ?? deckId}」`);
    } catch (error) {
        vocabError.set(`删除词书未完成：${vocabFailureText(error)}`);
    } finally {
        vocabBusy.set(false);
    }
}
function vocabScheduledText(minutes: number) {
    if (!Number.isFinite(minutes) || minutes <= 0) return "稍后";
    if (minutes < 60) return `${Math.round(minutes)} 分钟后`;
    if (minutes < 1440) return `${Math.round(minutes / 60)} 小时后`;
    return `${(minutes / 1440).toFixed(1)} 天后`;
}
async function gradeVocabCard(rating: string) {
    const card = vocabQueue()[0];
    // A grade is a self-assessment of a shown answer: the answer has to be
    // visible first, so a stray digit cannot rate a card the user never saw.
    if (!card || vocabBusy() || !vocabRevealed()) return;
    if (!vocabLive()) {
        vocabQueue.set(vocabQueue().filter((item) => item.cardId !== card.cardId));
        if (card.stage === "new")
            vocabRemainingNew.set(Math.max(0, vocabRemainingNew() - 1));
        else vocabRemainingDue.set(Math.max(0, vocabRemainingDue() - 1));
        decrementVocabDeck(card);
        setVocabRevealed(false);
        vocabOperation.set(null);
        vocabError.set("");
        vocabNotice.set(
            `已记录：${vocabRatingLabel(rating)} · 样例队列不写入 Core`,
        );
        return;
    }
    const previous = vocabOperation();
    const reuse =
        !!previous &&
        previous.cardId === card.cardId &&
        previous.rating === rating;
    const operation: VocabOperation =
        reuse && previous
            ? { ...previous, attempts: previous.attempts + 1 }
            : {
                  cardId: card.cardId,
                  rating,
                  id: vocabOperationId(card, rating),
                  attempts: 1,
              };
    vocabOperation.set(operation);
    vocabBusy.set(true);
    vocabError.set("");
    vocabNotice.set("");
    let failure = "";
    try {
        const value = vocabAnswer(
            await VocabHost.review(card.cardId, rating, operation.id),
        );
        vocabOperation.set(null);
        vocabRemainingDue.set(Number(value.remaining_due ?? vocabRemainingDue()));
        vocabRemainingNew.set(Number(value.remaining_new ?? vocabRemainingNew()));
        vocabQueue.set(vocabQueue().filter((item) => item.cardId !== card.cardId));
        setVocabRevealed(false);
        vocabNotice.set(
            `已记录：${vocabRatingLabel(rating)} · 下次 ${vocabScheduledText(
                Number(value.scheduled_minutes ?? 0),
            )}`,
        );
        await refreshVocabDecks();
        await loadVocabQueue(vocabDeck());
    } catch (error) {
        failure = vocabFailureText(error);
    } finally {
        vocabBusy.set(false);
    }
    if (!failure) return;
    if (operation.attempts >= 2) {
        useVocabFixture(
            `评分未能写入本机 Core（${failure}），已回退样例数据。`,
        );
        return;
    }
    vocabError.set(`评分未保存：${failure}`);
    vocabNotice.set("重试会复用同一次评分的操作编号，不会重复安排这张卡。");
}
function retryVocabReview() {
    const operation = vocabOperation();
    if (!operation) {
        vocabError.set("");
        return;
    }
    void gradeVocabCard(operation.rating);
}
let vocabBooted = false;
function bootVocabWorkspace() {
    if (vocabBooted) return;
    vocabBooted = true;
    void restoreVocabPreferences();
    setTimeout(() => void loadVocabDecks(), 0);
}
async function settleVocab() {
    for (let attempt = 0; attempt < 60 && vocabBusy(); attempt += 1)
        await new Promise((resolve) => setTimeout(resolve, 100));
}
/// Evidence-only, driven by App when the host was started with
/// QUSOR_WORKBENCH_EVIDENCE and the launcher asked for the vocabulary target.
export async function captureVocabEvidence() {
    let dir = "";
    try {
        const result = JSON.parse(await VocabHost.evidence());
        if (!result.ok || !result.value?.active) return;
        if (String(result.value.target ?? "") !== "vocab") return;
        dir = result.value.dir;
    } catch (_) {
        return;
    }
    if (!dir) return;
    for (let attempt = 0; attempt < 40 && !vocabReady(); attempt += 1)
        await new Promise((resolve) => setTimeout(resolve, 100));
    await new Promise((resolve) => setTimeout(resolve, 300));
    await VocabHost.capture(`${dir}/decks.png`);
    const deck = vocabDecks()[0];
    if (!deck) return;
    selectVocabDeck(deck.id);
    for (let attempt = 0; attempt < 40 && !currentVocabCard(); attempt += 1)
        await new Promise((resolve) => setTimeout(resolve, 100));
    await new Promise((resolve) => setTimeout(resolve, 300));
    await VocabHost.capture(`${dir}/study.png`);
    if (!currentVocabCard()) return;
    speakVocabWord();
    await new Promise((resolve) => setTimeout(resolve, 400));
    await VocabHost.capture(`${dir}/spoken.png`);
    chooseVocabAccent("uk");
    await new Promise((resolve) => setTimeout(resolve, 300));
    await VocabHost.capture(`${dir}/accent.png`);
    chooseVocabAccent("us");
    revealVocabAnswer();
    await new Promise((resolve) => setTimeout(resolve, 300));
    await VocabHost.capture(`${dir}/revealed.png`);
    await gradeVocabCard("good");
    await settleVocab();
    await new Promise((resolve) => setTimeout(resolve, 300));
    await VocabHost.capture(`${dir}/reviewed.png`);
    for (let round = 0; round < 40 && currentVocabCard(); round += 1) {
        revealVocabAnswer();
        await gradeVocabCard("good");
        await settleVocab();
    }
    await new Promise((resolve) => setTimeout(resolve, 300));
    await VocabHost.capture(`${dir}/done.png`);
}

function VocabDeckRow(props: { deck: VocabDeck; compact: boolean }) {
    const selected = () => vocabDeck() === props.deck.id;
    return (
        <View
            style={{
                borderRadius: 9,
                borderCurve: "continuous",
                borderWidth: props.compact ? 1 : 0,
                borderColor: ink(
                    selected() ? "border-control" : "border-subtle",
                ),
                backgroundColor: ink(
                    selected()
                        ? "surface-selected"
                        : props.compact
                          ? "surface-preview"
                          : "transparent",
                ),
                padding: props.compact ? 12 : 4,
                gap: props.compact ? 10 : 4,
            }}
        >
            <Button
                accessibilityLabel={`选择词书 ${props.deck.title}`}
                onPress={() => selectVocabDeck(props.deck.id)}
                style={{
                    minHeight: 44,
                    paddingHorizontal: 8,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 10,
                    borderRadius: 7,
                    borderCurve: "continuous",
                }}
                hoverStyle={{ backgroundColor: ink("surface-hover") }}
                activeStyle={{ backgroundColor: ink("surface-pressed") }}
                focusStyle={focus()}
            >
                <Glyph
                    name="book"
                    size={16}
                    color={ink(selected() ? "text-accent" : "text-secondary")}
                />
                <View style={{ flex: 1, gap: 2, minWidth: 0 }}>
                    <Copy size={props.compact ? 15 : 13} strong={props.compact}>
                        {props.deck.title}
                    </Copy>
                    <Copy size={11} muted>
                        {`共 ${props.deck.total} · 待复习 ${props.deck.due} · 新词 ${props.deck.new}`}
                    </Copy>
                </View>
            </Button>
            <View
                style={{
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 6,
                    paddingHorizontal: 8,
                }}
            >
                <If when={props.compact}>
                    <Tool
                        label={`开始学习 ${props.deck.title}`}
                        text="开始学习"
                        raised
                        size="touch"
                        onPress={() => selectVocabDeck(props.deck.id)}
                    />
                </If>
                <TextButton
                    label="词表"
                    accessibilityLabel={`查看 ${props.deck.title} 词表`}
                    size={props.compact ? "touch" : "compact"}
                    filled
                    onPress={() => void openVocabWordbookCards(props.deck.id)}
                />
                <TextButton
                    label="删除"
                    accessibilityLabel={`删除词书 ${props.deck.title}`}
                    size={props.compact ? "touch" : "compact"}
                    disabled={vocabBusy()}
                    onPress={() => void deleteVocabWordbook(props.deck.id)}
                />
            </View>
        </View>
    );
}

function VocabDeckList(props: { compact: boolean }) {
    return (
        <View
            accessibilityLabel="背单词词书导航"
            style={{
                width: props.compact ? "100%" : 228,
                flex: props.compact ? 1 : 0,
                flexShrink: 0,
                borderRightWidth: props.compact ? 0 : 1,
                borderColor: ink("border-subtle"),
                backgroundColor: ink(
                    props.compact ? "surface-base" : "surface-toolbar",
                ),
            }}
        >
            <View
                style={{
                    minHeight: 52,
                    paddingHorizontal: props.compact ? 16 : 14,
                    paddingVertical: 8,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 9,
                }}
            >
                <Badge icon="book" size={28} tone="surface-accent" />
                <View style={{ flex: 1, minWidth: 0 }}>
                    <Copy size={props.compact ? 16 : 13} strong>
                        词书与设置
                    </Copy>
                    <Copy size={11} muted>
                        {`${vocabDecks().length} 本词书 · ${vocabSourceDetail()}`}
                    </Copy>
                </View>
                <Tool
                    label="导入词书 JSON"
                    icon="plus"
                    text={props.compact ? "导入词书 JSON" : "导入"}
                    raised={props.compact}
                    size={props.compact ? "touch" : "compact"}
                    disabled={vocabBusy()}
                    onPress={() => void importVocabWordbook()}
                />
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }}>
                <View
                    style={{
                        padding: props.compact ? 16 : 10,
                        gap: props.compact ? 14 : 8,
                    }}
                >
                    <View
                        style={{
                            padding: 12,
                            gap: 10,
                            borderRadius: 9,
                            borderCurve: "continuous",
                            borderWidth: 1,
                            borderColor: ink("border-subtle"),
                            backgroundColor: ink("surface-inset"),
                        }}
                    >
                        <View
                            style={{
                                flexDirection: "row",
                                alignItems: "center",
                                flexWrap: "wrap",
                                gap: 6,
                            }}
                        >
                            <Copy size={12} strong>
                                每日新词上限
                            </Copy>
                            <View style={{ flex: 1 }} />
                            <Each of={[...VOCAB_DAILY_LIMIT_OPTIONS]}>
                                {(limit) => (
                                    <TextButton
                                        label={`${limit()}`}
                                        accessibilityLabel={`每日新词上限 ${limit()}`}
                                        size={props.compact ? "touch" : "compact"}
                                        raised={
                                            vocabDailyNewLimit() === limit()
                                        }
                                        filled={
                                            vocabDailyNewLimit() !== limit()
                                        }
                                        onPress={() =>
                                            void changeVocabDailyNewLimit(
                                                limit(),
                                            )
                                        }
                                    />
                                )}
                            </Each>
                        </View>
                        <View
                            style={{
                                flexDirection: "row",
                                alignItems: "center",
                                flexWrap: "wrap",
                                gap: 8,
                            }}
                        >
                            <Copy size={12} muted>
                                读音偏好
                            </Copy>
                            <View style={{ flex: 1 }} />
                            <TextButton
                                label={
                                    vocabAutoSpeak()
                                        ? "自动朗读：开"
                                        : "自动朗读：关"
                                }
                                size={props.compact ? "touch" : "compact"}
                                raised={vocabAutoSpeak()}
                                filled={!vocabAutoSpeak()}
                                onPress={() => void toggleVocabAutoSpeak()}
                            />
                            <TextButton
                                label={
                                    vocabSlowSpeak() ? "慢速：开" : "慢速：关"
                                }
                                size={props.compact ? "touch" : "compact"}
                                raised={vocabSlowSpeak()}
                                filled={!vocabSlowSpeak()}
                                onPress={() => void toggleVocabSlowSpeak()}
                            />
                        </View>
                    </View>
                    <If when={!!vocabNotice()}>
                        <Copy size={12} muted>
                            {vocabNotice()}
                        </Copy>
                    </If>
                    <If when={!!vocabError()}>
                        <Copy size={12} muted>
                            {vocabError()}
                        </Copy>
                    </If>
                    <If when={vocabDecks().length === 0}>
                        <View style={{ paddingVertical: 18 }}>
                            <Copy size={12} muted>
                                正在读取词书…
                            </Copy>
                        </View>
                    </If>
                    <View style={{ gap: props.compact ? 10 : 4 }}>
                        <Each
                            of={vocabDecks()}
                            key={(deck) => `vocab-deck-${deck.id}`}
                        >
                            {(deck) => (
                                <VocabDeckRow
                                    deck={deck()}
                                    compact={props.compact}
                                />
                            )}
                        </Each>
                    </View>
                </View>
            </Scroll>
            <If when={!props.compact}>
                <Rule />
                <View style={{ padding: 13, gap: 4 }}>
                    <Copy size={11} muted lines={2}>
                        评分由 FSRS-5 安排下次复习 · 卸载插件后学习数据仍在 Core
                    </Copy>
                    <Copy size={11} muted lines={2}>
                        音标来自词书或样例资源 · 读音由系统语音朗读
                    </Copy>
                </View>
            </If>
        </View>
    );
}

function VocabDeckPlaceholder() {
    return (
        <View
            style={{
                flexGrow: 1,
                alignItems: "center",
                justifyContent: "center",
                gap: 10,
                paddingVertical: 40,
            }}
        >
            <Badge icon="book" size={52} tone="surface-accent" />
            <Copy size={20} strong>
                选择一本词书
            </Copy>
            <Copy muted>
                {`共 ${vocabDecks().length} 本词书 · 来源：${vocabSource()}`}
            </Copy>
            <Copy size={11} muted>
                先清到期复习，再按今日新词预算学习
            </Copy>
        </View>
    );
}

function VocabDoneState() {
    return (
        <View
            style={{
                flexGrow: 1,
                alignItems: "center",
                justifyContent: "center",
                gap: 10,
                paddingVertical: 40,
            }}
        >
            <Badge icon="clock" size={52} tone="surface-accent" />
            <Copy size={22} strong>
                今天完成
            </Copy>
            <Copy muted>这本词书的队列已经清空，明天再来；不会自动扩大新词预算。</Copy>
            <Copy size={11} muted>
                {`来源：${vocabSource()}`}
            </Copy>
        </View>
    );
}

/// One accent of the sample reading. The chip only changes which IPA shows and
/// which system voice the speaker asks for; Core keeps none of it.
function VocabAccentChip(props: {
    accent: string;
    label: string;
    compact?: boolean;
}) {
    const selected = () => vocabAccent() === props.accent;
    return (
        <Button
            accessibilityLabel={props.label}
            onPress={() => chooseVocabAccent(props.accent)}
            style={{
                height: props.compact ? 44 : 24,
                paddingHorizontal: props.compact ? 14 : 8,
                alignItems: "center",
                justifyContent: "center",
                borderRadius: props.compact ? 10 : 6,
                borderCurve: "continuous",
                backgroundColor: ink(
                    selected() ? "surface-selected" : "transparent",
                ),
                borderWidth: 1,
                borderColor: ink(
                    selected() ? "border-control" : "border-subtle",
                ),
            }}
            hoverStyle={{ backgroundColor: ink("surface-hover") }}
            activeStyle={{ backgroundColor: ink("surface-pressed") }}
            focusStyle={focus()}
        >
            <Copy size={props.compact ? 13 : 11} muted={!selected()} strong={selected()}>
                {props.label}
            </Copy>
        </Button>
    );
}

/// The part of speech and the phonetic of the selected accent. A term the
/// sample books do not cover says so instead of showing an invented phonetic.
function VocabPhoneticRow(props: { compact: boolean }) {
    const card = () => currentVocabCard();
    const entry = () => pronunciationOf(card()?.term ?? "");
    const posText = () => card()?.pos || entry()?.pos || "";
    const phoneticText = () =>
        card()?.phonetic ||
        (entry() ? phoneticOf(card()?.term ?? "", vocabAccent()) : "");
    return (
        <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
            <If when={!!posText()}>
                <Copy size={props.compact ? 12 : 13} muted>
                    {posText()}
                </Copy>
            </If>
            <If when={!!phoneticText()}>
                <Copy size={props.compact ? 13 : 14} muted>
                    {phoneticText()}
                </Copy>
            </If>
            <If when={!posText() && !phoneticText() && !props.compact}>
                <Copy size={11} muted>
                    音标未收录 · 由词书资源提供
                </Copy>
            </If>
            <View style={{ flex: 1 }} />
            <VocabAccentChip accent="us" label="美音" compact={props.compact} />
            <VocabAccentChip accent="uk" label="英音" compact={props.compact} />
        </View>
    );
}

function VocabCardView(props: { compact: boolean }) {
    return (
        <View
            accessibilityLabel={`学习卡片 ${currentVocabCard()!.term}`}
            style={{
                alignSelf: "center",
                width: "100%",
                maxWidth: 720,
                padding: props.compact ? 20 : 26,
                gap: 14,
                borderWidth: 1,
                borderColor: ink("border-subtle"),
                borderRadius: 12,
                borderCurve: "continuous",
                backgroundColor: ink("surface-preview"),
            }}
        >
            <View
                style={{
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 8,
                }}
            >
                <Badge icon="book" size={26} />
                <Copy size={11} muted strong>
                    {vocabStageLabel(currentVocabCard()!.stage)}
                </Copy>
                <View style={{ flex: 1 }} />
                <Copy size={11} muted>
                    {`队列 ${vocabQueue().length} 张`}
                </Copy>
            </View>
            <View
                style={{
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 10,
                }}
            >
                <Copy size={props.compact ? 32 : 40} strong>
                    {currentVocabCard()!.term}
                </Copy>
                <Button
                    accessibilityLabel={`朗读 ${currentVocabCard()!.term}`}
                    onPress={speakVocabWord}
                    style={{
                        width: props.compact ? 44 : 36,
                        height: props.compact ? 44 : 36,
                        alignItems: "center",
                        justifyContent: "center",
                        borderRadius: 9,
                        borderCurve: "continuous",
                        backgroundColor: ink("surface-control"),
                        borderWidth: 1,
                        borderColor: ink(
                            vocabSpeaking()
                                ? "border-control"
                                : "border-subtle",
                        ),
                    }}
                    hoverStyle={{
                        backgroundColor: ink("surface-hover"),
                        borderColor: ink("border-control"),
                    }}
                    activeStyle={{ backgroundColor: ink("surface-pressed") }}
                    focusStyle={focus()}
                >
                    <Glyph
                        name="speaker"
                        size={18}
                        color={ink(
                            vocabSpeaking()
                                ? "text-accent"
                                : "text-secondary",
                        )}
                    />
                </Button>
            </View>
            <VocabPhoneticRow compact={props.compact} />
            <If when={!vocabRevealed()}>
                <Copy size={13} muted>
                    先回想这个词的意思，再显示答案。
                </Copy>
                <Copy size={11} muted>
                    释义、例句与四档评分在显示答案后出现。
                </Copy>
            </If>
            <If when={vocabRevealed()}>
                <Copy size={17}>{currentVocabCard()!.meaning}</Copy>
                <View style={{ flexDirection: "row", gap: 10 }}>
                    <View
                        style={{
                            width: 3,
                            borderRadius: 2,
                            backgroundColor: ink("border-control"),
                        }}
                    />
                    <View style={{ flex: 1, gap: 6 }}>
                        <Copy size={14} muted lines={3}>
                            {currentVocabCard()!.example}
                        </Copy>
                        <View style={{ flexDirection: "row" }}>
                            <TextButton
                                label="朗读例句"
                                size={props.compact ? "touch" : "compact"}
                                filled
                                onPress={speakVocabExample}
                            />
                        </View>
                    </View>
                </View>
            </If>
        </View>
    );
}

function VocabActionBar(props: { compact: boolean }) {
    return (
        <View style={{ flexShrink: 0 }}>
            <Rule />
            <If when={!vocabRevealed()}>
                <View
                    style={{
                        paddingHorizontal: props.compact ? 16 : 22,
                        paddingVertical: 12,
                        gap: 8,
                        backgroundColor: ink("surface-toolbar"),
                    }}
                >
                    <Button
                        accessibilityLabel="显示答案"
                        onPress={revealVocabAnswer}
                        style={{
                            height: props.compact ? 46 : 44,
                            alignItems: "center",
                            justifyContent: "center",
                            borderRadius: 8,
                            borderCurve: "continuous",
                            backgroundColor: ink("surface-accent"),
                            borderWidth: 1,
                            borderColor: ink("border-control"),
                        }}
                        hoverStyle={{ backgroundColor: ink("surface-hover") }}
                        activeStyle={{ backgroundColor: ink("surface-pressed") }}
                        focusStyle={focus()}
                    >
                        <Copy size={14} strong>
                            显示答案
                        </Copy>
                    </Button>
                    <If when={!props.compact}>
                        <Copy size={11} muted>
                            空格或回车显示答案 · R 朗读单词；只看词条，先自己回想
                        </Copy>
                    </If>
                </View>
            </If>
            <If when={vocabRevealed()}>
                <View
                    style={{
                        paddingHorizontal: props.compact ? 16 : 22,
                        paddingVertical: 12,
                        gap: 8,
                        backgroundColor: ink("surface-toolbar"),
                    }}
                >
                    <View style={{ flexDirection: "row", gap: 8 }}>
                        <Each of={vocabRatings}>
                            {(rating) => (
                                <Button
                                    accessibilityLabel={rating().label}
                                    disabled={vocabBusy()}
                                    onPress={() =>
                                        void gradeVocabCard(rating().rating)
                                    }
                                    style={{
                                        flex: 1,
                                        height: props.compact ? 46 : 44,
                                        justifyContent: "center",
                                        alignItems: "center",
                                        gap: 1,
                                        borderRadius: 8,
                                        borderCurve: "continuous",
                                        backgroundColor: ink("surface-control"),
                                        borderWidth: 1,
                                        borderColor: ink("border-subtle"),
                                        opacity: vocabBusy() ? 0.4 : 1,
                                    }}
                                    hoverStyle={{
                                        backgroundColor: ink("surface-hover"),
                                        borderColor: ink("border-control"),
                                    }}
                                    activeStyle={{
                                        backgroundColor: ink("surface-pressed"),
                                    }}
                                    focusStyle={focus()}
                                >
                                    <Copy size={13} strong>
                                        {rating().label}
                                    </Copy>
                                    <Copy size={10} muted>
                                        {rating().hint}
                                    </Copy>
                                </Button>
                            )}
                        </Each>
                    </View>
                    <If when={!props.compact}>
                        <Copy size={11} muted>
                            {vocabLive()
                                ? "评分写入本机 Core · 数字键 1–4 · R 朗读"
                                : "样例数据不写入 Core · 数字键 1–4 · R 朗读"}
                        </Copy>
                    </If>
                </View>
            </If>
        </View>
    );
}

function VocabWordsView(props: { compact: boolean }) {
    const deck = () => vocabDeckOf(vocabWordsDeckId());
    return (
        <View style={{ flex: 1, minWidth: 0 }}>
            <View
                style={{
                    height: 54,
                    paddingHorizontal: props.compact ? 14 : 22,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 10,
                    backgroundColor: ink("surface-base"),
                }}
            >
                <Tool
                    label="返回词书列表"
                    icon="back"
                    text="词书列表"
                    size={props.compact ? "touch" : "regular"}
                    onPress={() =>
                        vocabMobileView.set(props.compact ? "decks" : "study")
                    }
                />
                <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
                    <Copy strong>{deck()?.title ?? "词表"} · 全部词条</Copy>
                    <Copy size={11} muted>
                        {`共 ${vocabWordsList().length} 个词条`}
                    </Copy>
                </View>
                <If when={!!deck()}>
                    <Tool
                        label="开始学习此词书"
                        text="开始学习"
                        raised
                        size={props.compact ? "touch" : "compact"}
                        onPress={() => selectVocabDeck(vocabWordsDeckId())}
                    />
                </If>
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }}>
                <View style={{ padding: props.compact ? 16 : 22, gap: 10 }}>
                    <If when={vocabWordsLoading()}>
                        <Copy muted>正在读取词表…</Copy>
                    </If>
                    <Each
                        of={vocabWordsList()}
                        key={(item) => `word-row-${item.id}-${item.term}`}
                    >
                        {(item) => {
                            const sample = pronunciationOf(item().term);
                            const pos = item().pos || sample?.pos || "";
                            const phonetic =
                                item().phonetic ||
                                (sample
                                    ? phoneticOf(item().term, vocabAccent())
                                    : "");
                            return (
                                <View
                                    style={{
                                        padding: 14,
                                        gap: 6,
                                        borderRadius: 10,
                                        borderCurve: "continuous",
                                        borderWidth: 1,
                                        borderColor: ink("border-subtle"),
                                        backgroundColor: ink("surface-preview"),
                                    }}
                                >
                                    <View
                                        style={{
                                            flexDirection: "row",
                                            alignItems: "center",
                                            gap: 8,
                                        }}
                                    >
                                        <Copy size={16} strong>
                                            {item().term}
                                        </Copy>
                                        <If when={!!pos}>
                                            <Copy size={12} muted>
                                                {pos}
                                            </Copy>
                                        </If>
                                        <If when={!!phonetic}>
                                            <Copy size={12} muted>
                                                {phonetic}
                                            </Copy>
                                        </If>
                                        <View style={{ flex: 1 }} />
                                        <Tool
                                            label={`朗读 ${item().term}`}
                                            icon="speaker"
                                            size={props.compact ? "touch" : "compact"}
                                            onPress={() =>
                                                void speakVocabText(
                                                    item().term,
                                                    `「${item().term}」`,
                                                )
                                            }
                                        />
                                    </View>
                                    <Copy size={14}>{item().meaning}</Copy>
                                    <If when={!!item().example}>
                                        <Copy size={12} muted lines={2}>
                                            {item().example}
                                        </Copy>
                                    </If>
                                </View>
                            );
                        }}
                    </Each>
                </View>
            </Scroll>
        </View>
    );
}

function VocabStudy(props: { compact: boolean }) {
    return (
        <View style={{ flex: 1, minWidth: 0 }}>
            <View
                style={{
                    height: props.compact ? 54 : 58,
                    paddingHorizontal: props.compact ? 14 : 22,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 10,
                    backgroundColor: ink("surface-base"),
                }}
            >
                <If when={props.compact}>
                    <Tool
                        label="返回词书列表"
                        icon="back"
                        text="词书列表"
                        size="touch"
                        onPress={() => vocabMobileView.set("decks")}
                    />
                </If>
                <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
                    <Copy strong>
                        {vocabDeckOf(vocabDeck())?.title ?? "背单词"}
                    </Copy>
                    <Copy size={11} muted>
                        {`${vocabSource()} · ${vocabSourceDetail()}`}
                    </Copy>
                </View>
                <View style={{ alignItems: "flex-end", gap: 1 }}>
                    <Copy size={11} muted>
                        {`待复习 ${vocabRemainingDue()}`}
                    </Copy>
                    <Copy size={11} muted>
                        {`今日新词 ${vocabRemainingNew()}`}
                    </Copy>
                </View>
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }} contentStyle={{ flexGrow: 1 }}>
                <View
                    style={{
                        flexGrow: 1,
                        justifyContent: "center",
                        padding: props.compact ? 18 : 26,
                        gap: 14,
                    }}
                >
                    <If when={!!vocabError()}>
                        <View
                            style={{
                                alignSelf: "center",
                                width: "100%",
                                maxWidth: 720,
                                padding: 12,
                                flexDirection: "row",
                                alignItems: "center",
                                gap: 9,
                                borderWidth: 1,
                                borderColor: ink("border-subtle"),
                                borderRadius: 9,
                                borderCurve: "continuous",
                                backgroundColor: ink("surface-inset"),
                            }}
                        >
                            <Glyph
                                name="info"
                                size={16}
                                color={ink("text-accent")}
                            />
                            <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
                                <Copy size={12} lines={2}>
                                    {vocabError()}
                                </Copy>
                                <If when={!!vocabOperation()}>
                                    <Copy size={11} muted>
                                        重试复用同一次评分的操作编号。
                                    </Copy>
                                </If>
                            </View>
                            <If when={!!vocabOperation()}>
                                <TextButton
                                    label="重试评分"
                                    size={props.compact ? "touch" : "compact"}
                                    filled
                                    onPress={retryVocabReview}
                                />
                            </If>
                            <If when={!vocabOperation()}>
                                <TextButton
                                    label="重试读取"
                                    size={props.compact ? "touch" : "compact"}
                                    filled
                                    onPress={retryVocabDecks}
                                />
                            </If>
                        </View>
                    </If>
                    <If when={vocabLoading()}>
                        <View
                            style={{
                                flexGrow: 1,
                                alignItems: "center",
                                justifyContent: "center",
                                gap: 8,
                                paddingVertical: 40,
                            }}
                        >
                            <Copy muted>正在读取学习队列…</Copy>
                        </View>
                    </If>
                    <If when={!vocabLoading() && !vocabDeckOf(vocabDeck())}>
                        <VocabDeckPlaceholder />
                    </If>
                    <If
                        when={
                            !vocabLoading() &&
                            !!vocabDeckOf(vocabDeck()) &&
                            !currentVocabCard()
                        }
                    >
                        <VocabDoneState />
                    </If>
                    <If
                        when={
                            !vocabLoading() &&
                            !!vocabDeckOf(vocabDeck()) &&
                            !!currentVocabCard()
                        }
                    >
                        <VocabCardView compact={props.compact} />
                    </If>
                    <If when={!!vocabNotice()}>
                        <View
                            style={{
                                alignSelf: "center",
                                width: "100%",
                                maxWidth: 720,
                            }}
                        >
                            <Copy size={11} muted>
                                {vocabNotice()}
                            </Copy>
                        </View>
                    </If>
                </View>
            </Scroll>
            <If when={!vocabLoading() && !!currentVocabCard()}>
                <VocabActionBar compact={props.compact} />
            </If>
        </View>
    );
}

export function VocabWorkspace(props: { compact: boolean }) {
    bootVocabWorkspace();
    effect(() => {
        if (!props.compact || vocabMobileView() === "decks") return;
        BackHandler.on("press", () => {
            if (vocabMobileView() !== "decks") {
                vocabMobileView.set("decks");
                return true;
            }
            return false;
        });
    });
    keymap.bind({
        space: () => revealVocabAnswer(),
        enter: () => revealVocabAnswer(),
        r: () => speakVocabWord(),
        "1": () => void gradeVocabCard("again"),
        "2": () => void gradeVocabCard("hard"),
        "3": () => void gradeVocabCard("good"),
        "4": () => void gradeVocabCard("easy"),
    });
    return (
        <View
            accessibilityLabel="背单词插件工作区"
            style={{
                flex: 1,
                minWidth: 0,
                flexDirection: props.compact ? "column" : "row",
                backgroundColor: ink("surface-base"),
            }}
        >
            <If when={!props.compact}>
                <VocabDeckList compact={false} />
                <If when={vocabMobileView() === "words"}>
                    <VocabWordsView compact={false} />
                </If>
                <If when={vocabMobileView() !== "words"}>
                    <VocabStudy compact={false} />
                </If>
            </If>
            <If when={props.compact && vocabMobileView() === "decks"}>
                <VocabDeckList compact={true} />
            </If>
            <If when={props.compact && vocabMobileView() === "words"}>
                <VocabWordsView compact={true} />
            </If>
            <If when={props.compact && vocabMobileView() === "study"}>
                <VocabStudy compact={true} />
            </If>
        </View>
    );
}
