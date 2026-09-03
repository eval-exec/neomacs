const NAMED_KEY_SYMBOLS = new Map([
  ["Backspace", 0xff08],
  ["Tab", 0xff09],
  ["Enter", 0xff0d],
  ["Escape", 0xff1b],
  ["Home", 0xff50],
  ["ArrowLeft", 0xff51],
  ["ArrowUp", 0xff52],
  ["ArrowRight", 0xff53],
  ["ArrowDown", 0xff54],
  ["PageUp", 0xff55],
  ["PageDown", 0xff56],
  ["End", 0xff57],
  ["Insert", 0xff63],
  ["Delete", 0xffff],
]);

function modifierSample(event) {
  return {
    shift: event.shiftKey,
    control: event.ctrlKey,
    meta: event.altKey,
    super_: event.metaKey,
  };
}

function keySymbol(event) {
  if (NAMED_KEY_SYMBOLS.has(event.key)) return NAMED_KEY_SYMBOLS.get(event.key);
  if (event.key.length === 1) return event.key.codePointAt(0);
  const functionKey = /^F(\d{1,2})$/.exec(event.key);
  if (functionKey) {
    const number = Number(functionKey[1]);
    if (number >= 1 && number <= 35) return 0xffbd + number;
  }
  return null;
}

function isPlainTextKey(event) {
  return event.key.length === 1
    && !event.ctrlKey
    && !event.altKey
    && !event.metaKey;
}

function keyObservation(event, state, target) {
  if (event.isComposing || isPlainTextKey(event)) return null;
  const symbol = keySymbol(event);
  if (symbol === null) return null;
  return {
    type: "key",
    symbol,
    modifiers: modifierSample(event),
    state,
    target,
  };
}

/**
 * Connect DOM text services and lifecycle observations to the typed editor
 * input protocol. Printable text intentionally travels through the hidden
 * input element, not `keydown`, so an IME commit and an ordinary character
 * share one Unicode path and cannot be dispatched twice.
 */
export function installBrowserInput({
  root,
  textInput,
  enqueueInput,
  targetFrame,
  sendViewport,
}) {
  let composing = false;
  let closeRequested = false;

  const enqueue = (event) => enqueueInput([event]);
  const focusTextInput = () => textInput.focus({ preventScroll: true });
  const sendKey = (event, state) => {
    const observation = keyObservation(event, state, targetFrame());
    if (observation === null) return;
    event.preventDefault();
    enqueue(observation);
  };
  const requestClose = () => {
    if (closeRequested) return;
    closeRequested = true;
    enqueue({ type: "close-requested", target: targetFrame() });
  };

  root.addEventListener("keydown", (event) => sendKey(event, "pressed"), true);
  root.addEventListener("keyup", (event) => sendKey(event, "released"), true);
  root.addEventListener("resize", sendViewport);
  root.addEventListener("focus", () => enqueue({
    type: "focus-changed",
    focused: true,
    target: targetFrame(),
  }));
  root.addEventListener("blur", () => enqueue({
    type: "focus-changed",
    focused: false,
    target: targetFrame(),
  }));
  root.addEventListener("pointerdown", focusTextInput, true);
  root.addEventListener("pagehide", requestClose);
  root.addEventListener("beforeunload", requestClose);

  textInput.addEventListener("compositionstart", () => {
    composing = true;
  });
  textInput.addEventListener("compositionend", () => {
    composing = false;
  });
  textInput.addEventListener("input", (event) => {
    if (composing || event.isComposing) return;
    const text = textInput.value;
    textInput.value = "";
    if (text.length > 0) {
      enqueue({ type: "text-committed", text, target: targetFrame() });
    }
  });
}
