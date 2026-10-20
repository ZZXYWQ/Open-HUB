import { useId } from "react";

import { cn } from "@/lib/utils";

// Generic device renders drawn for this app (no vendor imagery).

function MouseArt() {
  const id = useId();
  return (
    <svg viewBox="0 0 200 340" className="h-full w-auto">
      <defs>
        <linearGradient id={`${id}a`} x1="0" x2="1">
          <stop offset="0" stopColor="#0b0b0d" />
          <stop offset=".45" stopColor="#2b2c31" />
          <stop offset=".55" stopColor="#27282d" />
          <stop offset="1" stopColor="#08080a" />
        </linearGradient>
        <radialGradient id={`${id}b`} cx=".5" cy=".3" r=".7">
          <stop offset="0" stopColor="#fff" stopOpacity=".12" />
          <stop offset="1" stopColor="#fff" stopOpacity="0" />
        </radialGradient>
        <radialGradient id={`${id}c`}>
          <stop offset="0" stopColor="#1196ff" />
          <stop offset="1" stopColor="#1196ff" stopOpacity="0" />
        </radialGradient>
      </defs>
      <path
        d="M100 8C152 8 184 50 186 122L188 238C188 302 150 334 100 334C50 334 12 302 12 238L14 122C16 50 48 8 100 8Z"
        fill={`url(#${id}a)`}
      />
      <path
        d="M100 8C152 8 184 50 186 122L188 238C188 302 150 334 100 334C50 334 12 302 12 238L14 122C16 50 48 8 100 8Z"
        fill={`url(#${id}b)`}
      />
      <path d="M14 132C52 146 148 146 186 132" stroke="#0a0a0b" strokeWidth="3" fill="none" />
      <path d="M100 10V138" stroke="#0a0a0b" strokeWidth="3" />
      <rect x="90" y="40" width="20" height="54" rx="10" fill="#0d0e10" />
      <rect x="94" y="46" width="12" height="42" rx="6" fill="#5b5f68" />
      <rect x="94" y="104" width="12" height="18" rx="4" fill="#202226" />
      <rect x="6" y="160" width="10" height="34" rx="4" fill="#2a2c31" />
      <rect x="6" y="200" width="10" height="34" rx="4" fill="#2a2c31" />
      <circle cx="100" cy="262" r="26" fill={`url(#${id}c)`} opacity=".55" />
      <circle cx="100" cy="262" r="7" fill="#1196ff" />
    </svg>
  );
}

function ReceiverArt() {
  const id = useId();
  return (
    <svg viewBox="0 0 120 200" className="h-full w-auto">
      <defs>
        <linearGradient id={id} x1="0" x2="1">
          <stop offset="0" stopColor="#2a2c31" />
          <stop offset=".5" stopColor="#4a4d55" />
          <stop offset="1" stopColor="#1c1d21" />
        </linearGradient>
      </defs>
      <rect x="38" y="20" width="44" height="70" rx="4" fill="#9aa0a8" />
      <rect x="46" y="32" width="10" height="10" fill="#555" />
      <rect x="64" y="32" width="10" height="10" fill="#555" />
      <rect x="28" y="84" width="64" height="96" rx="14" fill={`url(#${id})`} />
      <circle cx="60" cy="150" r="5" fill="#1196ff" />
    </svg>
  );
}

function KeyboardArt() {
  const id = useId();
  const keys = [];
  for (let r = 0; r < 5; r++) {
    for (let c = 0; c < 14; c++) {
      keys.push(<rect key={`${r}-${c}`} x={22 + c * 34} y={30 + r * 34} width="28" height="28" rx="4" fill="#2f3137" />);
    }
  }
  return (
    <svg viewBox="0 0 520 230" className="h-auto w-full">
      <defs>
        <linearGradient id={id} y2="1">
          <stop offset="0" stopColor="#3a3d44" />
          <stop offset="1" stopColor="#15161a" />
        </linearGradient>
      </defs>
      <rect x="6" y="14" width="508" height="200" rx="14" fill={`url(#${id})`} />
      {keys}
    </svg>
  );
}

export function DeviceArt({ kind, className }: { kind: string; className?: string }) {
  return (
    <div className={cn("flex items-center justify-center drop-shadow-[0_24px_28px_rgb(0_0_0/0.7)]", className)}>
      {kind === "Receiver" ? <ReceiverArt /> : kind === "Keyboard" ? <KeyboardArt /> : <MouseArt />}
    </div>
  );
}
