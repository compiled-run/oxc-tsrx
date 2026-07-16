type Item = { id: string; label: string };
export function Rows({ items }: { items: Item[] }) @{
  <ul>
    @for (item of items; index i; key item.id) {
      <li data-index={i}>{item.label}</li>;
    } @empty {
      <li>Empty</li>;
    }
  </ul>;
}
