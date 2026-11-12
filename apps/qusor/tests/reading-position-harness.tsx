import { mount, signal, Text, View } from "nsh";
import { Button } from "nsh/base";
import {
    loadReadingPosition,
    saveReadingPosition,
} from "../reading_position";

const documentId = 7;
const pageCount = 4;
const status = signal("loading");
const page = signal(0);

void (async () => {
    const position = await loadReadingPosition(documentId, pageCount);
    if (position) {
        page.set(position.page);
        status.set(`resumed ${position.page}`);
    } else {
        status.set("new");
    }
})();

async function next() {
    page.set(Math.min(pageCount - 1, page() + 1));
    const saved = await saveReadingPosition(documentId, page(), pageCount);
    status.set(saved ? `saved ${saved.page}` : "failed");
}

function App() {
    return (
        <View>
            <Text>{status()}</Text>
            <Button
                accessibilityLabel="next page"
                onPress={() => void next()}
            >
                <Text>next page</Text>
            </Button>
        </View>
    );
}

mount(App);
