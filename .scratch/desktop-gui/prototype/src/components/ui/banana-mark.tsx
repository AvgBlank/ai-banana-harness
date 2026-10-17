export function BananaMark({ size = 44 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 48 48" fill="none" aria-hidden>
      <path
        d="M9 13c1 18 13 28 31 21 2-.8 1-3-1-3-14 4-22-6-23-19-.2-2.500-7.200-2.500-7 1z"
        fill="var(--accent)"
        stroke="var(--peel)"
        strokeWidth="1.6"
        strokeLinejoin="round"
      />
      <path d="M9.500 12.500c-.4-3 1-5.500 3-6.500l1.500 4.500" stroke="var(--peel)" strokeWidth="1.600" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M16 20c2 7 7 11 14 11.500" stroke="var(--peel)" strokeOpacity=".45" strokeWidth="1.300" strokeLinecap="round" />
    </svg>
  );
}
