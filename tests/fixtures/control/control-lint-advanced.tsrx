type Props = { status: 0 | 1 };

export function Advanced({ status }: Props) @{
  <main>
    @switch (status) {
      @case 0: {
        var legacySwitch = 1;
        debugger;
        <p>{legacySwitch}</p>
      }
      @default: {
        <strong>Other</strong>
      }
    }
    @try {
      var legacyTry = 2;
      <p>{legacyTry}</p>
    } @pending {
      <span>Loading</span>
    } @catch (error, reset) {
      const catchUnused = error;
      <button onClick={reset}>Retry</button>
    }
  </main>
}
