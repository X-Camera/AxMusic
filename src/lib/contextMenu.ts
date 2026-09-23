/**
 * 全局屏蔽原生右键菜单。
 * 需要定制菜单的区域：容器加 `data-context-menu`，并在 onContextMenu 里 preventDefault + 弹自定义菜单。
 */
export function installContextMenuGuard(): void {
  document.addEventListener(
    "contextmenu",
    (e) => {
      e.preventDefault();
    },
    true,
  );
}
