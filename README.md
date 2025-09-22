# Wobbly Life Editor

A tool for editing Wobbly Life save files.

## Disclaimer

This software is provided "as is", without warranty of any kind, express or implied. Use at your own risk. The developers are not responsible for any data loss, corruption, save file damage, or any other issues that may occur from using this software. Always back up your save files before making any modifications.

By using this software, you acknowledge that you do so entirely at your own risk and that the authors and contributors of this software shall not be liable for any damages or losses of any kind.

## Finding Your Save Files

Your Wobbly Life save files are located at:

**Windows:**

```
%USERPROFILE%/AppData/LocalLow/RubberBandGames/Wobbly Life/Save/
```

To access this folder:

1. Press Windows + R
2. Type `%USERPROFILE%/AppData/LocalLow/RubberBandGames/Wobbly Life/Save/`
3. Press Enter
4. Under Steam ID folder, you'll find GameSave -folder which you can open with editor.

## Using the Editor

1. **Open the application** and click "Open GameSave Folder"
2. **Navigate to your save folder** (see path above)
3. **Select a save slot** from the left panel
4. **Choose the data type** to edit (Player Data, Mission Data, etc.)
5. **Use the search field** to find specific values:
   - Type "money" to find currency values
   - Type any text to search keys and values
6. **Press Enter** to jump to the first search result
7. **Edit values** in the bottom panel
8. **Click "Save to .sav"** to apply changes

## Important Notes

- Always enable "Zip backup on save" before making changes
- The "Create Backup Now" button creates an immediate backup
- Test your changes in-game to ensure they work correctly
- If something goes wrong, restore from your backup files

## Building from Source

```bash
cargo build --release
```

The GUI executable will be in `target/release/wle-gui.exe` (Windows) or `target/release/wle-gui` (other platforms).
