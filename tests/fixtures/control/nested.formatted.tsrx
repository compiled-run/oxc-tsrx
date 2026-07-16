type Item = { id: string; active: boolean };
export function Nested({ items }: { items: Item[] }) @{
  <main>
    @for (const item of items; key item.id) {
      @if (item.active) {
        <strong>{item.id}</strong>;
      } @else {
        <span>{item.id}</span>;
      }
    } @empty {
      <p>None</p>;
    }
  </main>;
}
