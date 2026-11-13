import { mount, signal, Text, View } from "nsh";
import { Button } from "nsh/base";
import {
    loadReadingZoom,
    nextReadingZoom,
    saveReadingZoom,
} from "../reading_view";

const zoom = signal(1);
const status = signal("loading");

void (async () => {
    zoom.set(await loadReadingZoom());
    status.set("ready");
})();

async function larger() {
    zoom.set(await saveReadingZoom(nextReadingZoom(zoom(), 1)));
    status.set("saved");
}

function App() {
    return (
        <View>
            <Text>{status()}</Text>
            <Text>{`zoom ${zoom()}`}</Text>
            <Button accessibilityLabel="larger page" onPress={() => void larger()}>
                <Text>larger page</Text>
            </Button>
        </View>
    );
}

mount(App);
