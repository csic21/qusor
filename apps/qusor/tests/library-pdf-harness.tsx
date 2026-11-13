import { mount, signal, Text, View } from "nsh";
import { LibraryHost } from "qusor/pdf-surface";
import { LibraryWorkspace } from "../contributions";
import { saveReadingPosition } from "../reading_position";

const fixture = Uint8Array.from(
    atob(
        "JVBERi0xLjcKJeLjz9MKMSAwIG9iago8PCAvVHlwZSAvQ2F0YWxvZyAvUGFnZXMgMiAwIFIgPj4KZW5kb2JqCjIgMCBvYmoKPDwgL1R5cGUgL1BhZ2VzIC9LaWRzIFs0IDAgUiA2IDAgUl0gL0NvdW50IDIgPj4KZW5kb2JqCjMgMCBvYmoKPDwgL1R5cGUgL0ZvbnQgL1N1YnR5cGUgL1R5cGUxIC9CYXNlRm9udCAvSGVsdmV0aWNhIC9FbmNvZGluZyAvV2luQW5zaUVuY29kaW5nID4+CmVuZG9iago0IDAgb2JqCjw8IC9UeXBlIC9QYWdlIC9QYXJlbnQgMiAwIFIgL01lZGlhQm94IFswIDAgNjEyIDc5Ml0gL0Nyb3BCb3ggWzE4IDI0IDU5NCA3NjhdIC9Sb3RhdGUgMCAvUmVzb3VyY2VzIDw8IC9Gb250IDw8IC9GMSAzIDAgUiA+PiA+PiAvQ29udGVudHMgNSAwIFIgPj4KZW5kb2JqCjUgMCBvYmoKPDwgL0xlbmd0aCAyMzAgPj4Kc3RyZWFtCkJUCi9GMSAxNCBUZgoyNCBUTAo3MiA3MDAgVGQKKFFVU09SIG5hdGl2ZSBQREYgcXVhbGlmaWNhdGlvbikgVGoKVCoKKFJlcGVhdGVkIGV2aWRlbmNlIHN0YXlzIGxvY2FsLikgVGoKVCoKKERpc3RpbmN0IGZpcnN0LXBhZ2UgY29udGV4dC4pIFRqClQqCihSZXBlYXRlZCBldmlkZW5jZSBzdGF5cyBsb2NhbC4pIFRqClQqCihTZWxlY3Rpb24gdGFyZ2V0OiBpbW11dGFibGUgb3JpZ2luYWxzLikgVGoKRVQKCmVuZHN0cmVhbQplbmRvYmoKNiAwIG9iago8PCAvVHlwZSAvUGFnZSAvUGFyZW50IDIgMCBSIC9NZWRpYUJveCBbMCAwIDc5MiA2MTJdIC9Dcm9wQm94IFszMCAxOCA3NjIgNTk0XSAvUm90YXRlIDkwIC9SZXNvdXJjZXMgPDwgL0ZvbnQgPDwgL0YxIDMgMCBSID4+ID4+IC9Db250ZW50cyA3IDAgUiA+PgplbmRvYmoKNyAwIG9iago8PCAvTGVuZ3RoIDE4OSA+PgpzdHJlYW0KQlQKL0YxIDE0IFRmCjI0IFRMCjcyIDUyMCBUZAooUVVTT1Igc2Vjb25kIHNvdXJjZSBwYWdlKSBUagpUKgooUmVwZWF0ZWQgZXZpZGVuY2Ugc3RheXMgbG9jYWwuKSBUagpUKgooU2Vjb25kLXBhZ2UgY29udGV4dCBpcyBleHBsaWNpdC4pIFRqClQqCihTZWxlY3Rpb24gdGFyZ2V0OiBpbW11dGFibGUgb3JpZ2luYWxzLikgVGoKRVQKCmVuZHN0cmVhbQplbmRvYmoKeHJlZgowIDgKMDAwMDAwMDAwMCA2NTUzNSBmIAowMDAwMDAwMDE1IDAwMDAwIG4gCjAwMDAwMDAwNjQgMDAwMDAgbiAKMDAwMDAwMDEyNyAwMDAwMCBuIAowMDAwMDAwMjI0IDAwMDAwIG4gCjAwMDAwMDAzODUgMDAwMDAgbiAKMDAwMDAwMDY2NiAwMDAwMCBuIAowMDAwMDAwODI4IDAwMDAwIG4gCnRyYWlsZXIKPDwgL1NpemUgOCAvUm9vdCAxIDAgUiA+PgpzdGFydHhyZWYKMTA2OAolJUVPRgo=",
    ),
    (character) => character.charCodeAt(0),
);
const ready = signal(false);
const error = signal("");
const title = `library-ui-fixture-${Date.now()}.pdf`;

void (async () => {
    const imported = JSON.parse(
        await LibraryHost.importPdf(title, fixture),
    );
    if (!imported.ok) {
        error.set(String(imported.error ?? "import failed"));
        return;
    }
    await saveReadingPosition(
        Number(imported.value.document_id),
        0,
        Number(imported.value.page_count),
    );
    ready.set(true);
})();

function App() {
    return (
        <View style={{ flex: 1 }}>
            <Text>{ready() ? `ready ${title}` : error() || "preparing"}</Text>
            <View style={{ flex: 1 }}>
                {ready() ? <LibraryWorkspace compact={false} /> : <View />}
            </View>
        </View>
    );
}

mount(App);
