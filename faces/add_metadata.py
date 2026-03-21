#!/usr/bin/env python3
"""Add tEXt metadata to all 28 bull face PNGs in faces/eink/."""

from PIL import Image
from PIL.PngImagePlugin import PngInfo
import os

FACES_DIR = os.path.join(os.path.dirname(__file__), 'eink')

FACE_METADATA = {
    'awake.png': {
        'Title': 'awake',
        'Description': 'Bull head with coffee mug, alert eyes',
        'Keywords': 'coffee, morning, alert, ready',
        'Trigger': 'Boot, wake from idle',
    },
    'happy.png': {
        'Title': 'happy',
        'Description': 'Grinning bull, wide smile, friendly',
        'Keywords': 'smiling, cheerful, content',
        'Trigger': 'Handshake captured',
    },
    'bored.png': {
        'Title': 'bored',
        'Description': 'Bull chewing grass/straw, side profile',
        'Keywords': 'grazing, idle, chewing, relaxed',
        'Trigger': 'No activity 0-10 epochs, SAFE mode',
    },
    'angry.png': {
        'Title': 'angry',
        'Description': 'Raging bull with electric sparks/lightning',
        'Keywords': 'furious, electric, storm, rage',
        'Trigger': '31-40 idle epochs',
    },
    'cool.png': {
        'Title': 'cool',
        'Description': 'Bull wearing sunglasses, confident smirk',
        'Keywords': 'sunglasses, chill, confident, swag',
        'Trigger': 'Capture variety cycle, night mode',
    },
    'excited.png': {
        'Title': 'excited',
        'Description': 'Bull roaring/bellowing, mouth wide open',
        'Keywords': 'roaring, yelling, pumped, wild',
        'Trigger': 'Many captures quickly, milestone',
    },
    'sad.png': {
        'Title': 'sad',
        'Description': 'Bull with tears, droopy eyes, rain cloud',
        'Keywords': 'crying, tearful, cloudy, down',
        'Trigger': '41+ idle epochs, low mood',
    },
    'sleep.png': {
        'Title': 'sleep',
        'Description': 'Full bull body lying down, Zzz floating',
        'Keywords': 'sleeping, resting, full body',
        'Trigger': '2am-5am quiet hours',
    },
    'intense.png': {
        'Title': 'intense',
        'Description': 'Bull snorting steam from nostrils, focused',
        'Keywords': 'snorting, steam, puffs, determined',
        'Trigger': 'Active attack streak',
    },
    'lonely.png': {
        'Title': 'lonely',
        'Description': 'Bull lying down alone, moon/stars',
        'Keywords': 'alone, nighttime, lying down',
        'Trigger': '11-20 idle epochs',
    },
    'smart.png': {
        'Title': 'smart',
        'Description': 'Bull with glasses, lightbulb above head',
        'Keywords': 'glasses, idea, lightbulb, thinking',
        'Trigger': '50th capture milestone',
    },
    'grateful.png': {
        'Title': 'grateful',
        'Description': 'Bull with bow/ribbon, gentle smile, eyes closed',
        'Keywords': 'bow, ribbon, thankful, gentle',
        'Trigger': '100th capture, capture variety cycle',
    },
    'motivated.png': {
        'Title': 'motivated',
        'Description': 'Bull charging with steam puffs',
        'Keywords': 'charging, steam, running, driven',
        'Trigger': 'Sunrise greeting, level up',
    },
    'demotivated.png': {
        'Title': 'demotivated',
        'Description': 'Bull head hanging low, defeated',
        'Keywords': 'head down, tired, given up',
        'Trigger': 'Very low mood (<0.1)',
    },
    'friend.png': {
        'Title': 'friend',
        'Description': 'Two bull heads touching noses, hearts',
        'Keywords': 'pair, kissing, hearts, love, peer',
        'Trigger': 'Pwngrid peer discovered',
    },
    'broken.png': {
        'Title': 'broken',
        'Description': 'Bull with bandages/cracks, hearts',
        'Keywords': 'damaged, hearts, hurt, cracked',
        'Trigger': 'System error, recovery exhausted',
    },
    'debug.png': {
        'Title': 'debug',
        'Description': 'Bull with monocle/goggles, examining',
        'Keywords': 'monocle, inspection, steampunk',
        'Trigger': 'Boot diagnostics',
    },
    'upload.png': {
        'Title': 'upload',
        'Description': 'Bull surrounded by binary 0s and 1s',
        'Keywords': 'binary, data, matrix, digital',
        'Trigger': 'WPA-SEC upload in progress',
    },
    'shutdown.png': {
        'Title': 'shutdown',
        'Description': 'Bull lying peacefully under moon and stars',
        'Keywords': 'night, moon, stars, peaceful',
        'Trigger': 'System shutdown',
    },
    'wifi_down.png': {
        'Title': 'wifi_down',
        'Description': 'Bull tangled in cables, WiFi X symbol',
        'Keywords': 'tangled, wires, broken wifi',
        'Trigger': 'WiFi interface lost',
    },
    'fw_crash.png': {
        'Title': 'fw_crash',
        'Description': 'Bull electrocuted, lightning bolts, dazed',
        'Keywords': 'shocked, zapped, sparks, crash',
        'Trigger': 'Firmware crash detected',
    },
    'ao_crashed.png': {
        'Title': 'ao_crashed',
        'Description': 'Mushroom cloud explosion, smoke columns',
        'Keywords': 'explosion, nuclear, smoke, disaster',
        'Trigger': 'AO process crashed',
    },
    'battery_critical.png': {
        'Title': 'battery_critical',
        'Description': 'Bull collapsed, empty battery icon',
        'Keywords': 'dead, collapsed, empty battery',
        'Trigger': 'Battery <5%',
    },
    'battery_low.png': {
        'Title': 'battery_low',
        'Description': 'Bull roaring/yawning, low energy',
        'Keywords': 'low energy, warning, depleted',
        'Trigger': 'Battery <20%',
    },
    'look_r.png': {
        'Title': 'look_r',
        'Description': 'Bull head facing right, neutral',
        'Keywords': 'profile, right, watching',
        'Trigger': 'Scanning right channels',
    },
    'look_l.png': {
        'Title': 'look_l',
        'Description': 'Bull head facing left, neutral',
        'Keywords': 'profile, left, watching',
        'Trigger': 'Scanning left channels',
    },
    'look_r_happy.png': {
        'Title': 'look_r_happy',
        'Description': 'Bull head facing right, slight smile',
        'Keywords': 'profile, right, happy',
        'Trigger': 'AP found while scanning right',
    },
    'look_l_happy.png': {
        'Title': 'look_l_happy',
        'Description': 'Bull head facing left, smile, wink',
        'Keywords': 'profile, left, happy, wink',
        'Trigger': 'AP found while scanning left',
    },
}

AUTHOR = 'Oxigotchi Project'


def add_metadata():
    updated = 0
    errors = []

    for filename, meta in sorted(FACE_METADATA.items()):
        filepath = os.path.join(FACES_DIR, filename)
        if not os.path.exists(filepath):
            errors.append(f'MISSING: {filename}')
            continue

        img = Image.open(filepath)

        # Create PngInfo with tEXt chunks
        png_info = PngInfo()
        png_info.add_text('Title', meta['Title'])
        png_info.add_text('Description', meta['Description'])
        png_info.add_text('Keywords', meta['Keywords'])
        png_info.add_text('Trigger', meta['Trigger'])
        png_info.add_text('Author', AUTHOR)

        # Save back with metadata preserved
        img.save(filepath, pnginfo=png_info)
        updated += 1
        print(f'  OK  {filename}')

    print(f'\nUpdated {updated}/{len(FACE_METADATA)} faces')
    if errors:
        print('Errors:')
        for e in errors:
            print(f'  {e}')


if __name__ == '__main__':
    add_metadata()
