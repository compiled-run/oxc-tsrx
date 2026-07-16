/// <reference path="./jsx.d.ts" />

type Item = {
  id: string;
  save(): Promise<void>;
};

declare const items: Item[];
declare const status: "ready" | "idle";
declare const Dynamic: (properties: { label: string }) => unknown;
declare function saveAll(): Promise<void>;

export function View() @{
  <main>
    @for (const item of items; index i; key item.id) {
      @if (i >= 0) {
        item.save();
        <span>{item.id}</span>
      }
    } @empty {
      <span>Empty</span>
    }
    @switch (status) {
      @case "ready": {
        @try {
          saveAll();
          <{Dynamic} label="saved" />
        } @pending {
          <span>Pending</span>
        } @catch (error, reset) {
          void error;
          <button onClick={reset}>Retry</button>
        }
      }
      @default: {
        <span>Idle</span>
      }
    }
    <style>.unused { color: red; }</style>
  </main>;
}
