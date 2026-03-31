# let rect = Rect::from_min_max(Pos2::new(1.0, 50.0), Pos2::new(10.0, 30.0));
# let camera = Camera::new(1.0, 2.0, 3.0);
import random

# for _ in range(10):
#     print(random.uniform(1.0, 10.0), random.uniform(30.0, 50.0))
# for _ in range(10):
#     print(random.uniform(1-3, 1+3), random.uniform(2-3, 2+3))
for _ in range(32):
    r = 32.0
    print(f"{random.uniform(-r, r):.3f},")
