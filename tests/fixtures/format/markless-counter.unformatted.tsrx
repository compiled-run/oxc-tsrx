// Derived from Markless poc/fixtures/proofs/ios-native-rendering-target/src/App.tsrx
import { state } from '@markless/core';

export function Counter() @{
let count=state(0);
<main><h1>Markless iOS Proof</h1><button onClick={()=>count++}>Count {count}</button></main>
}
