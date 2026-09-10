import { createContext, useContext, useEffect, useState } from "react";
import { Moon, Sun } from "lucide-react";
const ThemeContext = createContext({ theme: "dark", toggle: () => {} });
export function ThemeProvider({ children }) {
  const [theme, setTheme] = useState("dark");
  useEffect(() => {
    setTheme(document.documentElement.dataset.theme === "light" ? "light" : "dark");
    const sync = (event) => {
      if (event.key === "phaseforge.theme" && ["light","dark"].includes(event.newValue)) {
        setTheme(event.newValue); document.documentElement.dataset.theme = event.newValue;
      }
    };
    window.addEventListener("storage", sync);
    return () => window.removeEventListener("storage", sync);
  }, []);
  const toggle = () => {
    const next = theme === "dark" ? "light" : "dark";
    setTheme(next); document.documentElement.dataset.theme = next;
    try { localStorage.setItem("phaseforge.theme", next); } catch {}
  };
  return <ThemeContext.Provider value={{ theme, toggle }}>{children}</ThemeContext.Provider>;
}
export const useTheme = () => useContext(ThemeContext);
export function ThemeToggle({ label = false }) {
  const { theme, toggle } = useTheme();
  return <button type="button" className="themeToggle" onClick={toggle} title={`Switch to ${theme === "dark" ? "light" : "dark"} mode`} aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} mode`}>
    {theme === "dark" ? <Sun size={17} /> : <Moon size={17} />}{label && <span>{theme === "dark" ? "Light" : "Dark"} mode</span>}
  </button>;
}
