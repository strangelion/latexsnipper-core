"""Check RapidLayout model labels and run on complex fixture images."""
from rapid_layout import RapidLayout

engine = RapidLayout()

# Check class names (CDLA model labels)
result = engine(r"fixtures/complex.png")
print(f"Image: complex.png")
print(f"Class names: {result.class_names}")
print(f"Boxes: {len(result.boxes) if result.boxes else 0}")

if result.boxes:
    for i in range(len(result.boxes)):
        box = result.boxes[i]
        score = result.scores[i] if result.scores else 0
        name = result.class_names[i] if result.class_names else "?"
        print(f"  {i}: box={box} score={score:.3f} name={name[:20]}")

# Try text.png
result2 = engine(r"fixtures/text.png")
print(f"\nImage: text.png")
print(f"Boxes: {len(result2.boxes) if result2.boxes else 0}")
if result2.boxes:
    for i in range(min(5, len(result2.boxes))):
        print(f"  {i}: box={result2.boxes[i]} score={result2.scores[i]:.3f}")
