import unittest

from app.greet import greeting


class GreetingTest(unittest.TestCase):
    def test_greets(self):
        self.assertEqual(greeting(), "hello from python")


if __name__ == "__main__":
    unittest.main()
