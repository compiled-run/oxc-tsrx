type Guest = { id: string; name: string };

export function Greeting({ guests, ready }: { guests: Guest[]; ready: boolean }) @{
  @if (ready) {
    <ul style={{ padding: 0 }}>
      @for (const guest of guests; key guest.id) {
        <li>{guest.name}</li>;
      }
    </ul>;
  } @else {
    <p>Loading</p>;
  }
}
