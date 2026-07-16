type Props = { ready: boolean; label: string };
export function View({ ready, label }: Props) @{
  const pattern = /@if\s+\//gu;
  const message = `Crème 🚀 ${label}`;
  const contact = "@else@example.com";
  <p title={`@if ${label}`}>@if is text, not control</p>;
  /* @if (comment) {} */
  @if (ready) {
    <strong>{message}</strong>;
  } @else {
    <span>
      {pattern.source}
      {contact}
    </span>;
  }
}
