# weather/forecast

Two tools for the model, `weather.get_current` and `weather.get_forecast`, a tab in Settings with a few cities, and two settings: the units
(celsius or fahrenheit) and a default city, used when the user asks about the weather without naming one.

The figures are made up from the city's name and the date: the same city gives the same weather all day, whoever asks. The one function that
would call a weather service is `forecast_for()` in `main.py`; nothing else needs to change to make the plugin real.

Install: point the model at the `.tulpa` file, or add it on the Plugins page, switch it on and, if you like, fill in the settings.
Pack it again with `python3 plugin-sdk/python/pack.py plugin-sdk/examples/weather`.
