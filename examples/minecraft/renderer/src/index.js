export function createRenderer(ctx) {
  let iframe = null;

  function resolveViewerUrl() {
    const url = new URL(window.location.href);
    const fromQuery = url.searchParams.get("viewer");
    if (fromQuery && fromQuery.trim().length > 0) return fromQuery.trim();
    return "http://127.0.0.1:3007/";
  }

  function mountFrame() {
    if (iframe) return;
    const container = ctx.canvas?.parentElement;
    if (!container) return;

    ctx.canvas.style.display = "none";
    iframe = document.createElement("iframe");
    iframe.src = resolveViewerUrl();
    iframe.style.position = "absolute";
    iframe.style.inset = "0";
    iframe.style.width = "100%";
    iframe.style.height = "100%";
    iframe.style.border = "0";
    iframe.style.background = "#0b0d12";
    iframe.title = "Minecraft Viewer";

    container.style.position = "relative";
    container.appendChild(iframe);
  }

  return {
    mount() {
      mountFrame();
      ctx.log("info", "Mounted Minecraft viewer renderer");
    },
    unmount() {
      if (iframe && iframe.parentElement) iframe.parentElement.removeChild(iframe);
      iframe = null;
      if (ctx.canvas) ctx.canvas.style.display = "";
    },
    onResize() {},
    onState() {},
  };
}
