// Mock of `@tauri-apps/api/webview`: zoom is a no-op in the browser demo.
export function getCurrentWebview() {
  return {
    async setZoom(_scale: number) {},
    label: "main",
  };
}
