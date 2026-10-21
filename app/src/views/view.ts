/** 一个页面：`root` 挂到主区域，状态变化时调用 `update`，离开时调用 `dispose`。 */
export interface View {
  root: HTMLElement;
  update(): void;
  dispose?(): void;
  /** 离开前询问（比如有没发送的改动）；返回 false 留在本页。 */
  beforeLeave?(): Promise<boolean>;
}
