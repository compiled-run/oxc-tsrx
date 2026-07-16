type Props = { ready: boolean };

export function Branch({ ready }: Props) @{
  <section>
    @if (ready) {
      var legacy = 1;
      debugger;
      <p>{legacy}</p>
    } @else {
      const unused = 2;
      <span>no</span>
    }
  </section>
}
