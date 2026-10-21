/** 一个页面：`root` 挂到主区域，状态变化时调用 `update`，离开时调用 `dispose`。 */
export interface View {
  root: HTMLElement;
  update(): void;
  dispose?(): void;
}
