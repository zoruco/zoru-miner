import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("cleanup_launchers", Path(__file__).with_name("cleanup_launchers.py"))
mod = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(mod)


class LauncherCleanupTests(unittest.TestCase):
    def test_only_obsolete_local_zoru_shortcuts_are_archived(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            apps = root / "applications"
            apps.mkdir()
            archive = root / "retired"
            old = apps / "zoru-miner-v154.desktop"
            old.write_text("[Desktop Entry]\nType=Application\nName=ZORU Miner v1.5.4\nExec=zoru-miner\n")
            current = apps / "zoru-miner.desktop"
            current.write_text("[Desktop Entry]\nType=Application\nName=ZORU Miner\nExec=zoru-miner-gui\n")
            other = apps / "code.desktop"
            other.write_text("[Desktop Entry]\nType=Application\nName=Code\nExec=code\n")
            self.assertEqual(mod.cleanup(apps, archive, False), [old])
            self.assertTrue(old.exists())
            self.assertFalse(archive.exists())
            self.assertEqual(mod.cleanup(apps, archive, True), [old])
            self.assertFalse(old.exists())
            self.assertEqual(len(list(archive.iterdir())), 1)
            self.assertTrue(current.exists())
            self.assertTrue(other.exists())


if __name__ == "__main__":
    unittest.main()
