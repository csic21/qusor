// Research fixture: builds independently, but is not a QUSOR installed plugin.
import { host, mount, signal, Text, View } from "nsh";
import { Button, Input } from "nsh/base";

function App() {
  const input = signal("独立 TSX 界面");
  const status = signal("未调用宿主");
  const send = async () => {
    try {
      const reply = await host.call("qusor.probe.echo", { text: input() });
      status.set(JSON.stringify(reply));
    } catch (error) {
      status.set(String(error));
    }
  };
  return (
    <View style={{ padding: 24, gap: 16 }}>
      <Text>仓库外作者的原生 UI 构建探针</Text>
      <Input value={input()} onChange={({ value }) => input.set(value)} />
      <Button onPress={send}><Text>调用受控宿主接口</Text></Button>
      <Text>{status()}</Text>
    </View>
  );
}

mount(App);
