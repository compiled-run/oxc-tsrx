export function App() @{
  const label = "OXC TSRX BUILD";

  <main className="proof" data-proof={label}>
    @if (label) {
      <h1>{label}</h1>
    } @else {
      <h1>missing</h1>
    }

    <style>
      .proof {
        color: rebeccapurple;
      }
    </style>
  </main>
}
