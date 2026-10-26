import hashlib
import math
from datetime import date, timedelta
from typing import Annotated

from tulpa_plugin import BadArgs, Block, CallContext, Host, Plugin, Row, Section, Tab, run, tool

CONDITIONS = ["Clear", "Partly cloudy", "Cloudy", "Light rain", "Rain", "Thunderstorm", "Snow", "Fog", "Windy"]
DIRECTIONS = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"]
# The cities the tab lists besides the default one
TAB_CITIES = ["London", "Tokyo", "New York"]
MAX_DAYS = 7


def _number(city: str, day: date, what: str, low: int, high: int) -> int:
    """A number between low and high that is the same for the same city, date and what, whoever asks and whenever"""
    digest = hashlib.sha256(f"{city.strip().lower()}|{day.isoformat()}|{what}".encode()).digest()
    return low + int.from_bytes(digest[:4], "big") % (high - low + 1)


# Yearly mean temperature in celsius of cities people ask about; any other city gets one made up from its name
MEAN_CELSIUS = {
    "london": 11, "paris": 12, "berlin": 10, "madrid": 15, "rome": 16, "moscow": 6, "kyiv": 9, "istanbul": 15, "stockholm": 7,
    "new york": 13, "los angeles": 18, "chicago": 10, "toronto": 9, "mexico city": 17, "tokyo": 16, "seoul": 12, "beijing": 13,
    "shanghai": 17, "delhi": 25, "mumbai": 28, "dubai": 28, "cairo": 22, "lagos": 27, "nairobi": 19, "sydney": 18, "melbourne": 15,
    "cape town": 17, "sao paulo": 20, "buenos aires": 18,
}
# Where January is the middle of summer
SOUTHERN = {"sydney", "melbourne", "cape town", "sao paulo", "buenos aires"}
SEASON_SWING = 9


def forecast_for(city: str, day: date) -> dict:
    """The weather in a city on a day, in celsius and km/h. This is the one place a real weather service would be called."""
    key = city.strip().lower()
    mean = MEAN_CELSIUS.get(key, _number(city, date(2000, 1, 1), "climate", 2, 24))
    # Warmest in the middle of July, coldest in the middle of January, the other way round south of the equator
    season = -math.cos(2 * math.pi * (day.timetuple().tm_yday - 15) / 365) * (-1 if key in SOUTHERN else 1)
    # The mean is of the whole day and night, the high is the afternoon's
    high = round(mean + SEASON_SWING * season) + 3 + _number(city, day, "high", -3, 4)
    low = high - _number(city, day, "range", 3, 9)
    # A condition that fits the temperature: no snow in the heat, no thunderstorm in the cold
    fitting = [c for c in CONDITIONS if not (c == "Snow" and high > 3) and not (c == "Thunderstorm" and high < 8)]
    return {
        "high": high,
        "low": low,
        "condition": fitting[_number(city, day, "condition", 0, len(fitting) - 1)],
        "humidity": _number(city, day, "humidity", 35, 92),
        "wind_kmh": _number(city, day, "wind", 2, 38),
        "wind_from": DIRECTIONS[_number(city, day, "direction", 0, len(DIRECTIONS) - 1)],
        "rain_chance": _number(city, day, "rain", 0, 90),
    }


class Weather(Plugin):
    def __init__(self) -> None:
        self.fahrenheit = False
        self.default_city = ""
        self.host: Host | None = None

    async def configure(self, settings: dict, host: Host) -> None:
        units = str(settings.get("units") or "").strip().lower()
        if units not in ("", "c", "celsius", "f", "fahrenheit"):
            raise BadArgs("units must be celsius or fahrenheit")
        self.fahrenheit = units in ("f", "fahrenheit")
        self.default_city = str(settings.get("default_city") or "").strip()
        self.host = host

    def _temperature(self, celsius: int) -> int:
        return round(celsius * 9 / 5 + 32) if self.fahrenheit else celsius

    def _speed(self, kmh: int) -> dict:
        return {"speed": round(kmh / 1.609), "unit": "mph"} if self.fahrenheit else {"speed": kmh, "unit": "km/h"}

    def _city(self, city: str | None) -> str:
        city = (city or "").strip() or self.default_city
        if not city:
            raise BadArgs("no city was given and none is set as the default in the plugin's settings: ask the user which city")
        return city

    @property
    def _units(self) -> str:
        return "fahrenheit" if self.fahrenheit else "celsius"

    @tool("weather.get_current", "Gets the current weather in a city: the temperature, what it is like, the humidity and the wind. "
          "Use it when the user asks what the weather is now or today in some place.")
    async def get_current(self, ctx: CallContext, city: Annotated[str | None, "The city, for example Paris; the plugin's default city when left out"] = None) -> dict:
        city = self._city(city)
        now = forecast_for(city, date.today())
        # Between the day's low and its high
        temperature = now["low"] + (now["high"] - now["low"]) * 2 // 3
        await ctx.host.log(f"current weather in {city} for user {ctx.user_id}")
        return {
            "city": city,
            "units": self._units,
            "temperature": self._temperature(temperature),
            "feels_like": self._temperature(temperature - now["wind_kmh"] // 12),
            "condition": now["condition"],
            "humidity_percent": now["humidity"],
            "wind": {**self._speed(now["wind_kmh"]), "from": now["wind_from"]},
        }

    @tool("weather.get_forecast", "Gets the weather forecast for the next days in a city: each day's highest and lowest temperature, "
          "the conditions and the chance of rain. Use it when the user asks about tomorrow, the weekend or the coming days.")
    async def get_forecast(
        self,
        ctx: CallContext,
        city: Annotated[str | None, "The city, for example Paris; the plugin's default city when left out"] = None,
        days: Annotated[int, f"How many days from today, 1 to {MAX_DAYS}"] = 3,
    ) -> dict:
        city = self._city(city)
        if not 1 <= days <= MAX_DAYS:
            raise BadArgs(f"days must be between 1 and {MAX_DAYS}")
        await ctx.host.log(f"{days}-day forecast for {city} for user {ctx.user_id}")
        today = date.today()
        out = []
        for offset in range(days):
            day = today + timedelta(days=offset)
            weather = forecast_for(city, day)
            out.append({
                "date": day.isoformat(),
                "high": self._temperature(weather["high"]),
                "low": self._temperature(weather["low"]),
                "condition": weather["condition"],
                "rain_chance_percent": weather["rain_chance"],
            })
        return {"city": city, "units": self._units, "days": out}

    # The same text every turn while nothing it describes changes, so the model server keeps its cache of the chat
    async def prompt_section(self, user_id: int, tools: bool) -> str | None:
        if not tools:
            return None
        return ("The weather plugin is on: when the user asks about the weather in a place, call weather.get_current (now) or "
                "weather.get_forecast (the coming days) with the city instead of answering from memory.")

    def _tab(self) -> Tab:
        unit = "F" if self.fahrenheit else "C"
        cities = ([self.default_city] if self.default_city else []) + [c for c in TAB_CITIES if c.lower() != self.default_city.lower()]
        rows = []
        for city in cities:
            now = forecast_for(city, date.today())
            row = Row(city, city).subtitle(f"{self._temperature(now['high'])}°{unit}, {now['condition']}").button("forecast", "Forecast")
            rows.append(row.badge("default") if city == self.default_city else row)
        section = Section("Cities").block(Block.items("No cities.", rows))
        if not self.default_city:
            section.block(Block.hint("Set a default city in the plugin's settings and it comes first here and in answers that name no city."))
        return Tab().section(section)

    async def tab_layout(self, user_id: int) -> Tab:
        return self._tab()

    async def tab_action(self, user_id: int, action: str, args: dict) -> Tab:
        if action == "forecast":
            city = str(args.get("row") or "")
            unit = "F" if self.fahrenheit else "C"
            lines = []
            for offset in range(5):
                day = date.today() + timedelta(days=offset)
                weather = forecast_for(city, day)
                lines.append(f"{day.strftime('%a %d %b')}: {self._temperature(weather['low'])}…{self._temperature(weather['high'])}°{unit}, {weather['condition']}")
            return self._tab().popup(f"{city}: next 5 days", "\n".join(lines))
        raise BadArgs(f"no such action: {action}")


if __name__ == "__main__":
    run(Weather())
