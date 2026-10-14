// Word-by-word blur-in heading, adapted from the React Bits "BlurText" idea.
// Respects reduced motion by rendering the text immediately.
import { motion, useReducedMotion } from "motion/react";

export function BlurText({
  text,
  className,
  delay = 0.06,
  as: Tag = "h1",
}: {
  text: string;
  className?: string;
  delay?: number;
  as?: "h1" | "h2" | "p";
}) {
  const reduced = useReducedMotion();
  const words = text.split(" ");
  const MotionTag = motion[Tag];
  if (reduced) return <Tag className={className}>{text}</Tag>;
  return (
    <MotionTag className={className} aria-label={text}>
      {words.map((word, index) => (
        <motion.span
          // biome-ignore lint/suspicious/noArrayIndexKey: words are positional.
          key={index}
          aria-hidden="true"
          className="inline-block whitespace-pre"
          initial={{ opacity: 0, filter: "blur(8px)", y: 6 }}
          animate={{ opacity: 1, filter: "blur(0px)", y: 0 }}
          transition={{
            duration: 0.45,
            delay: index * delay,
            ease: [0.2, 0.8, 0.2, 1],
          }}
        >
          {word}
          {index < words.length - 1 ? " " : ""}
        </motion.span>
      ))}
    </MotionTag>
  );
}
