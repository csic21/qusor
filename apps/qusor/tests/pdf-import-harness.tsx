import { mount, signal, Text, View } from "nsh";
import { Button } from "nsh/base";
import { LibraryHost } from "qusor/pdf-surface";
import {
    parsePageGeometry,
    selectionBetween,
} from "../reading_selection";

const fixture = Uint8Array.from(
    atob(
        "JVBERi0xLjcKJeLjz9MKMSAwIG9iago8PCAvVHlwZSAvQ2F0YWxvZyAvUGFnZXMgMiAwIFIgPj4KZW5kb2JqCjIgMCBvYmoKPDwgL1R5cGUgL1BhZ2VzIC9LaWRzIFs0IDAgUiA2IDAgUl0gL0NvdW50IDIgPj4KZW5kb2JqCjMgMCBvYmoKPDwgL1R5cGUgL0ZvbnQgL1N1YnR5cGUgL1R5cGUxIC9CYXNlRm9udCAvSGVsdmV0aWNhIC9FbmNvZGluZyAvV2luQW5zaUVuY29kaW5nID4+CmVuZG9iago0IDAgb2JqCjw8IC9UeXBlIC9QYWdlIC9QYXJlbnQgMiAwIFIgL01lZGlhQm94IFswIDAgNjEyIDc5Ml0gL0Nyb3BCb3ggWzE4IDI0IDU5NCA3NjhdIC9Sb3RhdGUgMCAvUmVzb3VyY2VzIDw8IC9Gb250IDw8IC9GMSAzIDAgUiA+PiA+PiAvQ29udGVudHMgNSAwIFIgPj4KZW5kb2JqCjUgMCBvYmoKPDwgL0xlbmd0aCAyMzAgPj4Kc3RyZWFtCkJUCi9GMSAxNCBUZgoyNCBUTAo3MiA3MDAgVGQKKFFVU09SIG5hdGl2ZSBQREYgcXVhbGlmaWNhdGlvbikgVGoKVCoKKFJlcGVhdGVkIGV2aWRlbmNlIHN0YXlzIGxvY2FsLikgVGoKVCoKKERpc3RpbmN0IGZpcnN0LXBhZ2UgY29udGV4dC4pIFRqClQqCihSZXBlYXRlZCBldmlkZW5jZSBzdGF5cyBsb2NhbC4pIFRqClQqCihTZWxlY3Rpb24gdGFyZ2V0OiBpbW11dGFibGUgb3JpZ2luYWxzLikgVGoKRVQKCmVuZHN0cmVhbQplbmRvYmoKNiAwIG9iago8PCAvVHlwZSAvUGFnZSAvUGFyZW50IDIgMCBSIC9NZWRpYUJveCBbMCAwIDc5MiA2MTJdIC9Dcm9wQm94IFszMCAxOCA3NjIgNTk0XSAvUm90YXRlIDkwIC9SZXNvdXJjZXMgPDwgL0ZvbnQgPDwgL0YxIDMgMCBSID4+ID4+IC9Db250ZW50cyA3IDAgUiA+PgplbmRvYmoKNyAwIG9iago8PCAvTGVuZ3RoIDE4OSA+PgpzdHJlYW0KQlQKL0YxIDE0IFRmCjI0IFRMCjcyIDUyMCBUZAooUVVTT1Igc2Vjb25kIHNvdXJjZSBwYWdlKSBUagpUKgooUmVwZWF0ZWQgZXZpZGVuY2Ugc3RheXMgbG9jYWwuKSBUagpUKgooU2Vjb25kLXBhZ2UgY29udGV4dCBpcyBleHBsaWNpdC4pIFRqClQqCihTZWxlY3Rpb24gdGFyZ2V0OiBpbW11dGFibGUgb3JpZ2luYWxzLikgVGoKRVQKCmVuZHN0cmVhbQplbmRvYmoKeHJlZgowIDgKMDAwMDAwMDAwMCA2NTUzNSBmIAowMDAwMDAwMDE1IDAwMDAwIG4gCjAwMDAwMDAwNjQgMDAwMDAgbiAKMDAwMDAwMDEyNyAwMDAwMCBuIAowMDAwMDAwMjI0IDAwMDAwIG4gCjAwMDAwMDAzODUgMDAwMDAgbiAKMDAwMDAwMDY2NiAwMDAwMCBuIAowMDAwMDAwODI4IDAwMDAwIG4gCnRyYWlsZXIKPDwgL1NpemUgOCAvUm9vdCAxIDAgUiA+PgpzdGFydHhyZWYKMTA2OAolJUVPRgo=",
    ),
    (character) => character.charCodeAt(0),
);
const status = signal("idle");
const details = signal("");

async function importPdf() {
    const imported = JSON.parse(
        await LibraryHost.importPdf("native-fixture.pdf", fixture),
    );
    if (!imported.ok) {
        status.set(`import failed: ${imported.error}`);
        return;
    }
    const documentId = Number(imported.value.document_id);
    const page = await LibraryHost.pageImage(documentId, 0);
    const rotated = await LibraryHost.pageImage(documentId, 1);
    const info = JSON.parse(page.info);
    const geometry = page.geometry ? JSON.parse(page.geometry) : null;
    const text = JSON.parse(
        await LibraryHost.pageText(documentId, 0, 0, 256),
    );
    const captured = JSON.parse(
        await LibraryHost.captureSelection(
            documentId,
            0,
            0,
            5,
            "",
            `library-saved-${Date.now()}`,
        ),
    );
    const saved = JSON.parse(
        await LibraryHost.savedSelections(documentId, 0),
    );
    const parsed = parsePageGeometry(
        page.geometry,
        info.width,
        info.height,
    );
    const selection = parsed
        ? selectionBetween(
              parsed,
              String(text.value?.text ?? ""),
              0,
              Math.min(4, parsed.glyphs.length - 1),
          )
        : null;
    details.set(
        JSON.stringify({
            page_count: imported.value.page_count,
            image_ok: info.ok,
            image_width: info.width,
            file_bytes: page.file?.size ?? 0,
            glyphs_page1: geometry?.glyphs?.length ?? 0,
            glyphs_page2:
                typeof rotated.geometry === "string" && rotated.geometry
                    ? JSON.parse(rotated.geometry).glyphs?.length ?? 0
                    : 0,
            selected_text: selection?.text ?? "",
            saved_ok: captured.ok === true && saved.ok === true,
            saved_ranges: saved.value?.ranges ?? [],
            text_ok: text.ok,
            text: text.value?.text ?? "",
        }),
    );
    status.set("imported");
}

async function importBroken() {
    const imported = JSON.parse(
        await LibraryHost.importPdf(
            "broken.pdf",
            Uint8Array.from([37, 80, 68, 70, 45, 49]),
        ),
    );
    status.set(imported.ok ? "broken accepted" : "broken rejected");
}

function App() {
    return (
        <View>
            <Text>{status()}</Text>
            <Text>{details()}</Text>
            <Button
                accessibilityLabel="import pdf"
                onPress={() => void importPdf()}
            >
                <Text>import pdf</Text>
            </Button>
            <Button
                accessibilityLabel="import broken pdf"
                onPress={() => void importBroken()}
            >
                <Text>import broken pdf</Text>
            </Button>
        </View>
    );
}

mount(App);
