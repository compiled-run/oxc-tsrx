type Props = { items: string[]; ready: boolean };

export function Demo({ items, ready }: Props) @{
  const banned = items.length;
  debugger;

  @if (ready) {
    <ul class="items">
      @for (const item of items; key item) {
        <li>{item}</li>;
      }
    </ul>;
  } @else {
    <p>loading {banned}</p>;
  }
}
