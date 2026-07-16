type Props = { ready: boolean };

export function View({ ready }: Props) @{
  const contact = "@if@example.com";
  const unused = 1;
  var legacy = 2;
  // @if (false) { debugger; }
  @if (ready) {
    debugger;
    <main data-contact={contact}>{legacy}</main>;
  } @else {
    <aside>idle</aside>;
  }
}
