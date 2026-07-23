type Item = { id: string; label: string };
type Props = { ready: boolean; items: Item[] };

export function LintDemo({ ready, items }: Props) @{
  var legacyCount = 0;
  const neverRead = 'hover over my warning';
  console.log();

  @if (ready == true) {
    debugger;
    <button onClick={() => legacyCount++}>{items.length} items</button>;
  } @else {
    <p>Waiting</p>;
  }
}
