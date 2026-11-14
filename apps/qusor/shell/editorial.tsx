import { Scroll, Text, View, type JSX } from "nsh";
import { ink } from "../ui";

export function Paragraph(props: { children: JSX.Element }) {
    return (
        <Text
            style={{
                fontSize: 16,
                lineHeight: 29,
                fontWeight: "400",
                color: ink("text-primary"),
                userSelect: "text",
            }}
        >
            {props.children}
        </Text>
    );
}
export function Sheet(props: { children: JSX.Element }) {
    return (
        <Scroll style={{ flex: 1 }}>
            <View
                style={{
                    paddingHorizontal: 48,
                    paddingTop: 40,
                    paddingBottom: 70,
                    alignItems: "center",
                }}
            >
                <View style={{ width: "100%", maxWidth: 670, gap: 23 }}>
                    {props.children}
                </View>
            </View>
        </Scroll>
    );
}
