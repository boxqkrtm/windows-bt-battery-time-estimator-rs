from PIL import Image, ImageDraw
import os

# Create a 128x128 image with white background
img = Image.new('RGB', (128, 128), (255, 255, 255))  # White background for ICO
draw = ImageDraw.Draw(img)

# Draw battery body (green rectangle) - larger and more detailed
draw.rectangle([24, 40, 88, 72], fill=(76, 175, 80), outline=(46, 125, 50), width=3)

# Draw battery terminal (small rectangle on the right)
draw.rectangle([88, 48, 96, 64], fill=(46, 125, 50))

# Draw battery level indicator (lighter green)
draw.rectangle([32, 48, 80, 64], fill=(129, 199, 132))

# Draw battery charge segments for more detail
for i in range(3):
    x = 36 + i * 14
    draw.rectangle([x, 52, x + 10, 60], fill=(165, 214, 167))

# Draw Bluetooth symbol (larger and more detailed)
# Main Bluetooth shape
bt_center_x, bt_center_y = 64, 90
bt_size = 20

# Bluetooth symbol path (more detailed)
points = [
    (bt_center_x, bt_center_y - bt_size),  # top
    (bt_center_x + bt_size//2, bt_center_y - bt_size//2),  # top right
    (bt_center_x, bt_center_y),  # center
    (bt_center_x + bt_size//2, bt_center_y + bt_size//2),  # bottom right
    (bt_center_x, bt_center_y + bt_size),  # bottom
    (bt_center_x - bt_size//2, bt_center_y + bt_size//2),  # bottom left
    (bt_center_x, bt_center_y),  # center
    (bt_center_x - bt_size//2, bt_center_y - bt_size//2),  # top left
]

# Draw Bluetooth symbol background circle
draw.ellipse([bt_center_x - bt_size - 5, bt_center_y - bt_size - 5, 
              bt_center_x + bt_size + 5, bt_center_y + bt_size + 5], 
             fill=(33, 150, 243, 50))

# Draw Bluetooth symbol
draw.polygon(points, fill=(33, 150, 243))

# Draw vertical line through center
draw.rectangle([bt_center_x - 2, bt_center_y - bt_size, 
                bt_center_x + 2, bt_center_y + bt_size], fill=(33, 150, 243))

# Draw signal waves (larger and more visible)
wave_x, wave_y = 100, 30
for i in range(3):
    radius = 8 + i * 6
    draw.arc([wave_x - radius, wave_y - radius, wave_x + radius, wave_y + radius], 
             start=-45, end=45, fill=(33, 150, 243), width=3)

# Create smaller versions
img64 = img.resize((64, 64), Image.Resampling.LANCZOS)
img32 = img.resize((32, 32), Image.Resampling.LANCZOS)
img16 = img.resize((16, 16), Image.Resampling.LANCZOS)

# Save as ICO with multiple sizes including 128x128
img.save('icon.ico', format='ICO', sizes=[(128, 128), (64, 64), (32, 32), (16, 16)])
print("Large 128x128 icon created successfully!")

# Also save as PNG for verification
img.save('icon.png', format='PNG')
print("PNG version also created for verification!")

# Save individual sizes for verification
img64.save('icon_64.png', format='PNG')
img32.save('icon_32.png', format='PNG')
img16.save('icon_16.png', format='PNG')
print("All icon sizes created!") 